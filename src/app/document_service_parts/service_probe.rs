// @author kongweiguang

//! Probe single-flight state kept separate from registry and watcher methods.
//!
//! Keeping the blocking probe coordination here makes the UI-facing service
//! implementation small while preserving one shared `DocumentService` state.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gmark_document_core::LoadingPolicy;
use gmark_paged_document::OpenProbe;

use super::super::runtime::{file_key, normalize_path};
use super::super::types::DocumentServiceError;
use super::super::watcher::{ProbeKey, ProbeSlot, ProbeSlotState};
use super::{DocumentService, OPEN_WAIT_DEADLINE};

impl DocumentService {
    /// Single-flight the metadata probe/plan before a Resident body is read.
    /// Probe IO runs only in the Opening owner and is cached while the
    /// corresponding document lease/watcher remains alive.
    // 原因：同一路径只能让一个后台 owner 执行 probe，其他调用方必须观察同一成功或失败结果。
    pub(crate) fn probe_file<F, E>(
        &self,
        path: impl AsRef<Path>,
        policy: LoadingPolicy,
        loader: F,
    ) -> Result<OpenProbe, DocumentServiceError>
    where
        F: FnOnce(&Path, LoadingPolicy) -> Result<OpenProbe, E>,
        E: std::fmt::Display,
    {
        self.probe_file_with_deadline(path, policy, OPEN_WAIT_DEADLINE, loader)
    }

    /// 以有界等待复用同一路径的 probe；等待方超时会标记并移除当前代次，
    /// 但保留 owner 的 Arc 直到它结束，从而让迟到结果不会污染下一代打开。
    // 原因：慢盘或 UNC 路径不能让调用线程无限等待，超时后必须隔离迟到 owner 的结果。
    pub(crate) fn probe_file_with_deadline<F, E>(
        &self,
        path: impl AsRef<Path>,
        policy: LoadingPolicy,
        deadline: Duration,
        loader: F,
    ) -> Result<OpenProbe, DocumentServiceError>
    where
        F: FnOnce(&Path, LoadingPolicy) -> Result<OpenProbe, E>,
        E: std::fmt::Display,
    {
        let normalized_path = normalize_path(path.as_ref())?;
        let probe_key = ProbeKey {
            key: file_key(&normalized_path),
            max_resident_bytes: policy.effective_limits().max_resident_bytes,
            force_safe_source: policy.force_safe_source,
        };
        let loader_path = normalized_path.clone();
        let (slot, inserted) = {
            let mut probes = match self.probes.lock() {
                Ok(probes) => probes,
                Err(poisoned) => poisoned.into_inner(),
            };
            if let Some(slot) = probes.get(&probe_key) {
                (Arc::clone(slot), false)
            } else {
                let slot = Arc::new(ProbeSlot {
                    state: Mutex::new(ProbeSlotState::Opening),
                    ready: std::sync::Condvar::new(),
                });
                probes.insert(probe_key.clone(), Arc::clone(&slot));
                (slot, true)
            }
        };

        if inserted {
            let result = loader(&loader_path, policy)
                .map_err(|error| DocumentServiceError::OpenFailed(error.to_string()));
            let mut state = match slot.state.lock() {
                Ok(state) => state,
                Err(poisoned) => poisoned.into_inner(),
            };
            if matches!(&*state, ProbeSlotState::Abandoned) {
                drop(state);
                self.remove_probe_slot(&probe_key, &slot);
                return result;
            }
            match result {
                Ok(probe) => {
                    *state = ProbeSlotState::Ready(probe.clone());
                    slot.ready.notify_all();
                    Ok(probe)
                }
                Err(error) => {
                    let message = error.to_string();
                    *state = ProbeSlotState::Failed(message.clone());
                    slot.ready.notify_all();
                    drop(state);
                    let mut probes = match self.probes.lock() {
                        Ok(probes) => probes,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    if probes
                        .get(&probe_key)
                        .is_some_and(|candidate| Arc::ptr_eq(candidate, &slot))
                    {
                        probes.remove(&probe_key);
                    }
                    Err(DocumentServiceError::OpenFailed(message))
                }
            }
        } else {
            let started = Instant::now();
            let mut state = match slot.state.lock() {
                Ok(state) => state,
                Err(poisoned) => poisoned.into_inner(),
            };
            loop {
                match &*state {
                    ProbeSlotState::Ready(probe) => return Ok(probe.clone()),
                    ProbeSlotState::Failed(message) => {
                        return Err(DocumentServiceError::OpenFailed(message.clone()));
                    }
                    ProbeSlotState::Opening => {
                        let Some(remaining) = deadline.checked_sub(started.elapsed()) else {
                            drop(state);
                            self.abandon_probe_slot(&probe_key, &slot);
                            return Err(DocumentServiceError::OpenFailed(
                                "timed out waiting for document probe".to_owned(),
                            ));
                        };
                        let (next_state, wait_result) =
                            match slot.ready.wait_timeout(state, remaining) {
                                Ok(result) => result,
                                Err(poisoned) => poisoned.into_inner(),
                            };
                        state = next_state;
                        if wait_result.timed_out() && matches!(&*state, ProbeSlotState::Opening) {
                            drop(state);
                            self.abandon_probe_slot(&probe_key, &slot);
                            return Err(DocumentServiceError::OpenFailed(
                                "timed out waiting for document probe".to_owned(),
                            ));
                        }
                    }
                    ProbeSlotState::Abandoned => {
                        return Err(DocumentServiceError::OpenFailed(
                            "timed out waiting for document probe".to_owned(),
                        ));
                    }
                }
            }
        }
    }

    /// Mark a timed-out probe generation unusable before removing its map entry;
    /// an old owner can then finish without publishing a result into a new slot.
    // 原因：删除 map 不能阻止仍运行的 owner 回写，先标记代次才能隔离迟到结果。
    fn abandon_probe_slot(&self, probe_key: &ProbeKey, slot: &Arc<ProbeSlot>) {
        let mut state = match slot.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        if matches!(&*state, ProbeSlotState::Opening) {
            *state = ProbeSlotState::Abandoned;
            slot.ready.notify_all();
        }
        drop(state);
        self.remove_probe_slot(probe_key, slot);
    }

    /// Remove only the exact probe generation so a late owner cannot remove a
    /// newer request that reused the same normalized path.
    // 原因：同路径重试必须保留新代次的单飞槽位，不能被旧 owner 清理。
    fn remove_probe_slot(&self, probe_key: &ProbeKey, slot: &Arc<ProbeSlot>) {
        let mut probes = match self.probes.lock() {
            Ok(probes) => probes,
            Err(poisoned) => poisoned.into_inner(),
        };
        if probes
            .get(probe_key)
            .is_some_and(|candidate| Arc::ptr_eq(candidate, slot))
        {
            probes.remove(probe_key);
        }
    }
}
