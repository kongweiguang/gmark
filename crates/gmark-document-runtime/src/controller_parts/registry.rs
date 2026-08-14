// @author kongweiguang

//! Registry key、并发打开和 Save As 临时占用。

use super::super::*;

impl DocumentRegistryKey {
    /// 规范化路径作为共享 key；Windows 不区分大小写，因此统一比较形式。
    pub fn for_file(identity: &FileIdentity) -> Self {
        #[cfg(target_os = "windows")]
        {
            Self::File(PathBuf::from(
                identity
                    .canonical_path
                    .as_os_str()
                    .to_string_lossy()
                    .to_lowercase(),
            ))
        }
        #[cfg(not(target_os = "windows"))]
        {
            Self::File(identity.canonical_path.clone())
        }
    }
}

impl Default for DocumentRegistry {
    fn default() -> Self {
        Self {
            inner: Arc::new(RegistryInner {
                documents: Mutex::new(BTreeMap::new()),
            }),
        }
    }
}

impl RegistryInner {
    /// 只在最后租约释放后删除同一 handle 的槽位，避免旧句柄误删新打开的文档。
    pub(super) fn remove_if_unleased(
        &self,
        key: &DocumentRegistryKey,
        handle: &DocumentHandle,
    ) -> Result<bool, ControllerError> {
        let _gate = handle.lock_lease_gate();
        self.remove_if_unleased_locked(key, handle)
    }

    /// Remove a slot while the handle lifecycle gate is already held.
    /// Release and registry-open both use this locked form to keep the lease
    /// count and the registry map in one atomic lifecycle transition.
    pub(super) fn remove_if_unleased_locked(
        &self,
        key: &DocumentRegistryKey,
        handle: &DocumentHandle,
    ) -> Result<bool, ControllerError> {
        if handle.lease_count() != 0 {
            return Ok(false);
        }
        let mut documents = self
            .documents
            .lock()
            .map_err(|_| ControllerError::Poisoned)?;
        let Some(slot) = documents.get(key).cloned() else {
            return Ok(false);
        };
        let state = slot.state.lock().map_err(|_| ControllerError::Poisoned)?;
        let matches = matches!(&*state, RegistrySlotState::Ready(registered) if Arc::ptr_eq(&registered.0, &handle.0));
        drop(state);
        if matches {
            documents.remove(key);
            handle.clear_registry_binding();
        }
        Ok(matches)
    }
}

impl DocumentRegistry {
    /// Production callers share one deadline so a missing opener cannot pin a registry slot
    /// indefinitely while tests and specialized callers can still choose a shorter bound.
    pub const DEFAULT_OPEN_TIMEOUT: Duration = Duration::from_secs(30);

    /// 默认给共享打开设置有限等待，避免 UI 或后台打开任务永久卡在失联的 owner 上。
    pub fn open_or_insert_leased(
        &self,
        key: DocumentRegistryKey,
        create: impl FnOnce() -> Result<DocumentController, ControllerError>,
    ) -> Result<(DocumentHandle, DocumentLease, RegistryOpen), ControllerError> {
        self.open_or_insert_leased_with_timeout(key, Self::DEFAULT_OPEN_TIMEOUT, create)
    }

    /// Shared opening only permits one creator; bounded waiting makes a stalled creator observable
    /// and gives every waiter the same terminal error instead of leaving an `Opening` slot forever.
    pub fn open_or_insert_leased_with_timeout(
        &self,
        key: DocumentRegistryKey,
        timeout: Duration,
        create: impl FnOnce() -> Result<DocumentController, ControllerError>,
    ) -> Result<(DocumentHandle, DocumentLease, RegistryOpen), ControllerError> {
        let mut create = Some(create);
        loop {
            let (slot, owner) = {
                let mut documents = self
                    .inner
                    .documents
                    .lock()
                    .map_err(|_| ControllerError::Poisoned)?;
                match documents.get(&key).cloned() {
                    Some(slot) => (slot, false),
                    None => {
                        let slot = Arc::new(RegistrySlot {
                            state: Mutex::new(RegistrySlotState::Opening),
                            ready: Condvar::new(),
                        });
                        documents.insert(key.clone(), slot.clone());
                        (slot, true)
                    }
                }
            };

            if owner {
                let Some(create) = create.take() else {
                    return Err(ControllerError::open_failed(
                        "registry opening owner lost its create closure",
                    ));
                };
                let result = create();
                return match result {
                    Ok(controller) => {
                        let handle = DocumentHandle::new(controller);
                        let lease = handle.lease();
                        let mut state = slot.state.lock().map_err(|_| ControllerError::Poisoned)?;
                        match &*state {
                            RegistrySlotState::Opening => {
                                if let Err(error) =
                                    handle.attach_registry(Arc::downgrade(&self.inner), key.clone())
                                {
                                    *state = RegistrySlotState::Failed(error.clone());
                                    slot.ready.notify_all();
                                    return Err(error);
                                }
                                *state = RegistrySlotState::Ready(handle.clone());
                                slot.ready.notify_all();
                                Ok((handle, lease, RegistryOpen::Inserted))
                            }
                            RegistrySlotState::Failed(error) => Err(error.clone()),
                            RegistrySlotState::Ready(_) | RegistrySlotState::Reserved(_) => {
                                Err(ControllerError::open_failed(
                                    "registry opening slot changed before publication",
                                ))
                            }
                        }
                    }
                    Err(error) => {
                        let mut state = slot.state.lock().map_err(|_| ControllerError::Poisoned)?;
                        match &*state {
                            RegistrySlotState::Opening => {
                                *state = RegistrySlotState::Failed(error.clone());
                                slot.ready.notify_all();
                                Err(error)
                            }
                            RegistrySlotState::Failed(existing) => Err(existing.clone()),
                            RegistrySlotState::Ready(_) | RegistrySlotState::Reserved(_) => {
                                Err(error)
                            }
                        }
                    }
                };
            }

            let mut state = slot.state.lock().map_err(|_| ControllerError::Poisoned)?;
            match &*state {
                RegistrySlotState::Ready(handle) => {
                    let handle = handle.clone();
                    drop(state);
                    if let Some(lease) = self.lease_ready_handle(&key, &slot, &handle)? {
                        return Ok((handle, lease, RegistryOpen::Existing));
                    }
                    continue;
                }
                RegistrySlotState::Reserved(_) => {
                    return Err(ControllerError::KeyReserved(key.clone()));
                }
                RegistrySlotState::Failed(_) => {
                    // A caller arriving after the failed opening may retry.  Waiters that
                    // observed Opening take the branch below and receive the shared error.
                    drop(state);
                    let mut documents = self
                        .inner
                        .documents
                        .lock()
                        .map_err(|_| ControllerError::Poisoned)?;
                    if documents
                        .get(&key)
                        .is_some_and(|registered| Arc::ptr_eq(registered, &slot))
                    {
                        documents.remove(&key);
                    }
                    continue;
                }
                RegistrySlotState::Opening => {
                    let started = std::time::Instant::now();
                    while matches!(&*state, RegistrySlotState::Opening) {
                        let remaining = timeout.saturating_sub(started.elapsed());
                        if remaining.is_zero() {
                            let error = ControllerError::OpenTimedOut {
                                key: key.clone(),
                                timeout_ms: timeout.as_millis().min(u64::MAX as u128) as u64,
                            };
                            *state = RegistrySlotState::Failed(error.clone());
                            slot.ready.notify_all();
                            drop(state);
                            self.remove_slot_if_matches(&key, &slot)?;
                            return Err(error);
                        }
                        let (next_state, wait_result) = slot
                            .ready
                            .wait_timeout(state, remaining)
                            .map_err(|_| ControllerError::Poisoned)?;
                        state = next_state;
                        if wait_result.timed_out() && matches!(&*state, RegistrySlotState::Opening)
                        {
                            let error = ControllerError::OpenTimedOut {
                                key: key.clone(),
                                timeout_ms: timeout.as_millis().min(u64::MAX as u128) as u64,
                            };
                            *state = RegistrySlotState::Failed(error.clone());
                            slot.ready.notify_all();
                            drop(state);
                            self.remove_slot_if_matches(&key, &slot)?;
                            return Err(error);
                        }
                    }
                    match &*state {
                        RegistrySlotState::Ready(handle) => {
                            let handle = handle.clone();
                            drop(state);
                            if let Some(lease) = self.lease_ready_handle(&key, &slot, &handle)? {
                                return Ok((handle, lease, RegistryOpen::Existing));
                            }
                            continue;
                        }
                        RegistrySlotState::Failed(error) => return Err(error.clone()),
                        RegistrySlotState::Reserved(_) => {
                            return Err(ControllerError::KeyReserved(key.clone()));
                        }
                        RegistrySlotState::Opening => unreachable!(),
                    }
                }
            }
        }
    }

    /// Removing only the exact failed slot lets a timed-out owner finish without deleting a newer
    /// opening attempt that reused the same document key.
    fn remove_slot_if_matches(
        &self,
        key: &DocumentRegistryKey,
        slot: &Arc<RegistrySlot>,
    ) -> Result<(), ControllerError> {
        let mut documents = self
            .inner
            .documents
            .lock()
            .map_err(|_| ControllerError::Poisoned)?;
        if documents
            .get(key)
            .is_some_and(|registered| Arc::ptr_eq(registered, slot))
        {
            documents.remove(key);
        }
        Ok(())
    }

    /// Acquire a Ready-slot lease while holding the handle gate before taking
    /// the registry map.  This lock order matches last-lease removal and closes
    /// the lookup-to-lease race that could otherwise create two Controllers.
    fn lease_ready_handle(
        &self,
        key: &DocumentRegistryKey,
        slot: &Arc<RegistrySlot>,
        handle: &DocumentHandle,
    ) -> Result<Option<DocumentLease>, ControllerError> {
        let _gate = handle.lock_lease_gate();
        let documents = self
            .inner
            .documents
            .lock()
            .map_err(|_| ControllerError::Poisoned)?;
        let Some(current) = documents.get(key) else {
            return Ok(None);
        };
        if !Arc::ptr_eq(current, slot) {
            return Ok(None);
        }
        let state = slot.state.lock().map_err(|_| ControllerError::Poisoned)?;
        if matches!(&*state, RegistrySlotState::Ready(registered) if Arc::ptr_eq(&registered.0, &handle.0))
        {
            return Ok(Some(handle.lease_locked()));
        }
        Ok(None)
    }

    /// 保留兼容入口；新调用方应持有显式租约以表达视图生命周期。
    pub fn open_or_insert(
        &self,
        key: DocumentRegistryKey,
        create: impl FnOnce() -> Result<DocumentController, ControllerError>,
    ) -> Result<(DocumentHandle, RegistryOpen), ControllerError> {
        let (handle, lease, open) = self.open_or_insert_leased(key, create)?;
        drop(lease);
        Ok((handle, open))
    }

    /// 为 Save As 目标创建暂时占用，提交前其它打开操作只能看到 KeyReserved。
    pub fn reserve_save_as(
        &self,
        source: &DocumentHandle,
        target: DocumentRegistryKey,
    ) -> Result<SaveAsReservation, ControllerError> {
        let mut documents = self
            .inner
            .documents
            .lock()
            .map_err(|_| ControllerError::Poisoned)?;
        if let Some(slot) = documents.get(&target).cloned() {
            let state = slot.state.lock().map_err(|_| ControllerError::Poisoned)?;
            return match &*state {
                RegistrySlotState::Reserved(_) => Err(ControllerError::KeyReserved(target)),
                RegistrySlotState::Opening => Err(ControllerError::KeyReserved(target)),
                RegistrySlotState::Ready(_) => Err(ControllerError::KeyOccupied(target)),
                RegistrySlotState::Failed(_) => Err(ControllerError::KeyOccupied(target)),
            };
        }
        documents.insert(
            target.clone(),
            Arc::new(RegistrySlot {
                state: Mutex::new(RegistrySlotState::Reserved(source.clone())),
                ready: Condvar::new(),
            }),
        );
        Ok(SaveAsReservation {
            registry: Arc::downgrade(&self.inner),
            target,
            source: source.clone(),
            committed: false,
        })
    }

    /// 对已有目标返回一个真实 lease；目标空闲时才创建 reservation。
    pub fn reserve_save_as_outcome(
        &self,
        source: &DocumentHandle,
        target: DocumentRegistryKey,
    ) -> Result<SaveAsReserveOutcome, ControllerError> {
        let slot = self
            .inner
            .documents
            .lock()
            .map_err(|_| ControllerError::Poisoned)?
            .get(&target)
            .cloned();
        if let Some(slot) = slot {
            let state = slot.state.lock().map_err(|_| ControllerError::Poisoned)?;
            return match &*state {
                RegistrySlotState::Ready(handle) => Ok(SaveAsReserveOutcome::Occupied {
                    handle: handle.clone(),
                    lease: handle.lease(),
                }),
                RegistrySlotState::Reserved(_) | RegistrySlotState::Opening => {
                    Err(ControllerError::KeyReserved(target))
                }
                RegistrySlotState::Failed(_) => Err(ControllerError::KeyOccupied(target)),
            };
        }
        self.reserve_save_as(source, target)
            .map(SaveAsReserveOutcome::Reserved)
    }

    /// 兼容旧生命周期调用，仅在句柄没有租约时移除匹配 key。
    pub fn release_if_unused(
        &self,
        key: &DocumentRegistryKey,
        handle: &DocumentHandle,
    ) -> Result<bool, ControllerError> {
        self.inner.remove_if_unleased(key, handle)
    }
}

impl SaveAsReservation {
    /// 释放尚未提交的目标占用，令失败的 Save As 可安全重试。
    pub fn release(mut self) {
        if self.committed {
            return;
        }
        if let Some(registry) = self.registry.upgrade()
            && let Ok(mut documents) = registry.documents.lock()
            && let Some(slot) = documents.get(&self.target).cloned()
            && let Ok(state) = slot.state.lock()
        {
            let matches = matches!(&*state, RegistrySlotState::Reserved(handle) if Arc::ptr_eq(&handle.0, &self.source.0));
            drop(state);
            if matches {
                documents.remove(&self.target);
            }
        }
        self.committed = true;
    }

    /// 将目标槽位转为 source handle，并删除旧路径 key，保证 Save As 后只有新 key 可打开。
    pub fn commit(mut self) -> Result<DocumentHandle, ControllerError> {
        let registry = self
            .registry
            .upgrade()
            .ok_or(ControllerError::SaveAsReservationMissing)?;
        let mut documents = registry
            .documents
            .lock()
            .map_err(|_| ControllerError::Poisoned)?;
        let slot = documents
            .get(&self.target)
            .cloned()
            .ok_or(ControllerError::SaveAsReservationMissing)?;
        let mut state = slot.state.lock().map_err(|_| ControllerError::Poisoned)?;
        let valid = matches!(&*state, RegistrySlotState::Reserved(handle) if Arc::ptr_eq(&handle.0, &self.source.0));
        if !valid {
            return Err(ControllerError::SaveAsReservationMissing);
        }
        let source = self.source.clone();
        *state = RegistrySlotState::Ready(source.clone());
        drop(state);
        slot.ready.notify_all();
        let stale_keys = documents
            .iter()
            .filter_map(|(key, slot)| {
                if key == &self.target {
                    return None;
                }
                let state = slot.state.lock().ok()?;
                matches!(&*state, RegistrySlotState::Ready(handle) if Arc::ptr_eq(&handle.0, &source.0))
                    .then(|| key.clone())
            })
            .collect::<Vec<_>>();
        for key in stale_keys {
            documents.remove(&key);
        }
        source.attach_registry(Arc::downgrade(&registry), self.target.clone())?;
        self.committed = true;
        Ok(source)
    }
}

impl Drop for SaveAsReservation {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        // Drop cannot report an error; release is deliberately best-effort and
        // the explicit method remains available to callers that need ownership.
        if let Some(registry) = self.registry.upgrade()
            && let Ok(mut documents) = registry.documents.lock()
            && let Some(slot) = documents.get(&self.target).cloned()
            && let Ok(state) = slot.state.lock()
        {
            let matches = matches!(&*state, RegistrySlotState::Reserved(handle) if Arc::ptr_eq(&handle.0, &self.source.0));
            drop(state);
            if matches {
                documents.remove(&self.target);
            }
        }
        self.committed = true;
    }
}
