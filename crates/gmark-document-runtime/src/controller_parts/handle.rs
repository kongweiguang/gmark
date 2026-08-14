// @author kongweiguang

//! DocumentHandle、租约和保存状态回调的生命周期实现。

use super::super::*;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::Ordering;

impl WeakDocumentHandle {
    /// 将弱引用升级为句柄，避免回调或后台任务延长文档生命周期。
    pub fn upgrade(&self) -> Option<DocumentHandle> {
        self.0.upgrade().map(DocumentHandle)
    }
}

impl SaveStateCallbackRegistration {
    /// 只移除本注册项一次，让 watcher 可以安全地显式结束监听。
    pub fn unregister(&self) -> Result<bool, ControllerError> {
        if self.active.swap(false, Ordering::AcqRel) {
            let Some(inner) = self.handle.upgrade() else {
                return Ok(false);
            };
            let mut callbacks = inner
                .save_state_callbacks
                .lock()
                .map_err(|_| ControllerError::Poisoned)?;
            Ok(callbacks.remove(&self.id).is_some())
        } else {
            Ok(false)
        }
    }
}

impl Drop for SaveStateCallbackRegistration {
    fn drop(&mut self) {
        let _ = self.unregister();
    }
}

impl DocumentHandle {
    /// 把 Controller 放入共享内层，使 clone 只复制能力而不复制正文状态。
    pub fn new(controller: DocumentController) -> Self {
        Self(Arc::new(DocumentHandleInner {
            controller: Mutex::new(controller),
            leases: AtomicUsize::new(0),
            lease_gate: Mutex::new(()),
            registry: Mutex::new(None),
            last_lease_callback: Mutex::new(None),
            save_state_callbacks: Mutex::new(BTreeMap::new()),
            next_save_callback_id: AtomicUsize::new(1),
            shared_extensions: Mutex::new(HashMap::new()),
        }))
    }

    pub fn downgrade(&self) -> WeakDocumentHandle {
        WeakDocumentHandle(Arc::downgrade(&self.0))
    }

    /// Read a type-erased adapter extension without copying the Controller or
    /// allowing a transient view to become its lifetime owner.
    pub fn shared_extension<T>(&self) -> Result<Option<Arc<T>>, ControllerError>
    where
        T: Any + Send + Sync + 'static,
    {
        let extensions = self
            .0
            .shared_extensions
            .lock()
            .map_err(|_| ControllerError::Poisoned)?;
        let Some(extension) = extensions.get(&TypeId::of::<T>()).cloned() else {
            return Ok(None);
        };
        extension.downcast::<T>().map(Some).map_err(|_| {
            ControllerError::Mutation("shared document extension type mismatch".into())
        })
    }

    /// Install one adapter extension atomically; concurrent views receive the
    /// already-installed value and therefore cannot create duplicate workers.
    pub fn install_shared_extension<T>(&self, extension: Arc<T>) -> Result<Arc<T>, ControllerError>
    where
        T: Any + Send + Sync + 'static,
    {
        let mut extensions = self
            .0
            .shared_extensions
            .lock()
            .map_err(|_| ControllerError::Poisoned)?;
        if let Some(existing) = extensions.get(&TypeId::of::<T>()).cloned() {
            return existing.downcast::<T>().map_err(|_| {
                ControllerError::Mutation("shared document extension type mismatch".into())
            });
        }
        extensions.insert(TypeId::of::<T>(), extension.clone());
        Ok(extension)
    }

    pub fn lock(&self) -> Result<MutexGuard<'_, DocumentController>, ControllerError> {
        self.0
            .controller
            .lock()
            .map_err(|_| ControllerError::Poisoned)
    }

    pub fn lease_count(&self) -> usize {
        self.0.leases.load(Ordering::Acquire)
    }

    /// 新建租约必须和最后租约判定共享同一 gate，避免 discard 与打开窗口交错。
    pub fn lease(&self) -> DocumentLease {
        let _gate = self.lock_lease_gate();
        self.lease_locked()
    }

    /// Lock the lifecycle gate before inspecting or changing the global lease count.
    /// Registry publication and last-lease removal use the same order so a new
    /// view cannot be admitted between the zero-count check and slot removal.
    pub(super) fn lock_lease_gate(&self) -> std::sync::MutexGuard<'_, ()> {
        match self.0.lease_gate.lock() {
            Ok(gate) => gate,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Increment a lease count while the caller already owns `lease_gate`.
    /// Keeping this helper separate prevents registry lookup from re-locking the
    /// non-reentrant gate while it atomically validates and acquires a lease.
    pub(super) fn lease_locked(&self) -> DocumentLease {
        self.0.leases.fetch_add(1, Ordering::AcqRel);
        DocumentLease {
            handle: self.clone(),
            released: AtomicBool::new(false),
        }
    }

    /// 捕获当前保存队列状态供 watcher 进行代次判定。
    pub fn save_state(&self) -> Result<SaveStateNotification, ControllerError> {
        let controller = self.lock()?;
        Ok(SaveStateNotification {
            in_flight_revision: controller.save_in_flight_revision(),
            pending_revision: controller.save_pending_revision(),
        })
    }

    pub fn save_in_flight_revision(&self) -> Result<Option<DocumentRevision>, ControllerError> {
        Ok(self.lock()?.save_in_flight_revision())
    }

    pub fn save_pending_revision(&self) -> Result<Option<DocumentRevision>, ControllerError> {
        Ok(self.lock()?.save_pending_revision())
    }

    /// 注册保存状态观察者；回调只接收不可变通知，避免把 Controller 锁交给外部代码。
    pub fn register_save_state_callback(
        &self,
        callback: Arc<dyn Fn(SaveStateNotification) + Send + Sync>,
    ) -> Result<SaveStateCallbackRegistration, ControllerError> {
        let id = self.0.next_save_callback_id.fetch_add(1, Ordering::Relaxed);
        self.0
            .save_state_callbacks
            .lock()
            .map_err(|_| ControllerError::Poisoned)?
            .insert(id, callback);
        Ok(SaveStateCallbackRegistration {
            handle: Arc::downgrade(&self.0),
            id,
            active: AtomicBool::new(true),
        })
    }

    /// 通知回调时释放内部 map 锁，并吞掉外部 panic，避免破坏文档主锁。
    fn notify_save_state(&self) {
        let callbacks = match self.0.save_state_callbacks.lock() {
            Ok(callbacks) => callbacks.values().cloned().collect::<Vec<_>>(),
            Err(poisoned) => poisoned.into_inner().values().cloned().collect::<Vec<_>>(),
        };
        let state = match self.save_state() {
            Ok(state) => state,
            Err(_) => return,
        };
        for callback in callbacks {
            let _ = panic::catch_unwind(AssertUnwindSafe(|| callback(state)));
        }
    }

    /// 请求保存只修改队列；完成/失败之后再广播，避免 watcher 将入队误判为落盘。
    pub fn request_save_snapshot(&self) -> Result<Option<DocumentSaveSnapshot>, ControllerError> {
        self.lock()?.request_save_snapshot()
    }

    pub fn complete_save(
        &self,
        revision: DocumentRevision,
        identity: FileIdentity,
    ) -> Result<Option<DocumentSaveSnapshot>, ControllerError> {
        let promoted = self.lock()?.complete_save(revision, identity)?;
        self.notify_save_state();
        Ok(promoted)
    }

    pub fn fail_save(
        &self,
        revision: DocumentRevision,
        code: SaveFailureCode,
    ) -> Result<Option<DocumentSaveSnapshot>, ControllerError> {
        let result = self.lock()?.fail_save(revision, code)?;
        self.notify_save_state();
        Ok(result)
    }

    pub fn accept_external_append(
        &self,
        expected_revision: DocumentRevision,
        expected_identity: FileIdentity,
        source: FileSource,
        index: LineIndex,
        identity: FileIdentity,
    ) -> Result<(), ControllerError> {
        self.lock()?.accept_external_append(
            expected_revision,
            expected_identity,
            source,
            index,
            identity,
        )
    }

    pub fn reload_prepared_document(
        &self,
        expected_revision: DocumentRevision,
        expected_identity: FileIdentity,
        prepared: DocumentSession,
    ) -> Result<(), ControllerError> {
        self.lock()?
            .reload_prepared_document(expected_revision, expected_identity, prepared)
    }

    pub fn set_encoding(
        &self,
        view_id: DocumentViewInstanceId,
        transaction_id: TransactionId,
        encoding: TextEncoding,
    ) -> Result<DocumentRevision, ControllerError> {
        self.lock()?.set_encoding(view_id, transaction_id, encoding)
    }

    /// 只有调用方持有的租约正好等于当前总数时才允许 discard，避免清空其它窗口的编辑。
    pub fn discard_current_changes(&self) -> Result<bool, ControllerError> {
        self.discard_current_changes_for_owned_leases(1)
    }

    pub fn discard_current_changes_for_owned_leases(
        &self,
        owned_leases: usize,
    ) -> Result<bool, ControllerError> {
        let _gate = self
            .0
            .lease_gate
            .lock()
            .map_err(|_| ControllerError::Poisoned)?;
        if self.lease_count() != owned_leases {
            return Err(ControllerError::SharedDocumentStillLeased);
        }
        let mut controller = self.lock()?;
        let expected_revision = DocumentRevision(controller.session().revision());
        controller.discard_changes(expected_revision)
    }

    pub fn next_transaction_id(&self) -> Result<TransactionId, ControllerError> {
        Ok(self.lock()?.next_transaction_id())
    }

    pub fn register_last_lease_callback(
        &self,
        callback: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<(), ControllerError> {
        let mut current = self
            .0
            .last_lease_callback
            .lock()
            .map_err(|_| ControllerError::Poisoned)?;
        if current.is_some() {
            return Err(ControllerError::LastLeaseCallbackRegistered);
        }
        *current = Some(callback);
        Ok(())
    }

    /// Registry 绑定与句柄本身分离，确保 clone handle 不会延长 registry key 生命周期。
    pub(super) fn attach_registry(
        &self,
        registry: Weak<RegistryInner>,
        key: DocumentRegistryKey,
    ) -> Result<(), ControllerError> {
        *self
            .0
            .registry
            .lock()
            .map_err(|_| ControllerError::Poisoned)? = Some((registry, key));
        Ok(())
    }

    pub(super) fn registry_binding(
        &self,
    ) -> Result<Option<(Weak<RegistryInner>, DocumentRegistryKey)>, ControllerError> {
        Ok(self
            .0
            .registry
            .lock()
            .map_err(|_| ControllerError::Poisoned)?
            .clone())
    }

    pub(super) fn clear_registry_binding(&self) {
        if let Ok(mut binding) = self.0.registry.lock() {
            *binding = None;
        }
    }

    pub(super) fn invoke_last_lease_callback(&self) {
        let callback = self
            .0
            .last_lease_callback
            .lock()
            .ok()
            .and_then(|mut callback| callback.take());
        if let Some(callback) = callback {
            let _ = panic::catch_unwind(AssertUnwindSafe(|| callback()));
        }
    }
}

impl DocumentLease {
    pub fn handle(&self) -> DocumentHandle {
        self.handle.clone()
    }

    pub fn lease_count(&self) -> usize {
        self.handle.lease_count()
    }

    /// 复制租约代表一个新的真实视图所有者，而不是普通句柄 clone。
    pub fn clone_lease(&self) -> DocumentLease {
        self.handle.lease()
    }

    fn release(&self) {
        if self.released.swap(true, Ordering::AcqRel) {
            return;
        }
        // The gate must cover decrement, registry validation, and removal.  A
        // matching open therefore either acquires its lease first or observes
        // the slot gone and creates a new owner; it can never become active on
        // a handle that the registry has already removed.
        let gate = self.handle.lock_lease_gate();
        let previous = self.handle.0.leases.fetch_sub(1, Ordering::AcqRel);
        if previous == 0 {
            self.handle.0.leases.store(0, Ordering::Release);
            drop(gate);
            return;
        }
        if previous != 1 {
            drop(gate);
            return;
        }
        let binding = self.handle.registry_binding().ok().flatten();
        if let Some((registry, key)) = binding
            && let Some(registry) = registry.upgrade()
        {
            let _ = registry.remove_if_unleased_locked(&key, &self.handle);
        }
        drop(gate);
        self.handle.invoke_last_lease_callback();
    }
}

impl Clone for DocumentLease {
    fn clone(&self) -> Self {
        self.clone_lease()
    }
}

impl Drop for DocumentLease {
    fn drop(&mut self) {
        self.release();
    }
}
