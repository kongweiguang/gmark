// @author kongweiguang

//! 事件日志订阅与创建时的状态快照。

use super::super::*;

impl DocumentController {
    /// 在同一把 Controller 锁下捕获正文和 event sequence，避免订阅首事件丢失。
    fn state_snapshot(&self) -> DocumentStateSnapshot {
        DocumentStateSnapshot {
            document_id: self.document_id,
            revision: DocumentRevision(self.session.revision()),
            dirty: self.session.dirty,
            identity: self.session.file_identity.clone(),
            sequence: self.event_sequence,
            save_in_flight_revision: self.save_in_flight_revision(),
            save_pending_revision: self.save_pending_revision(),
            save: self.session.save_snapshot(),
        }
    }
}

impl DocumentHandle {
    /// 返回快照与游标，使后续 poll 只观察创建之后的事件。
    pub fn subscribe_with_snapshot(
        &self,
    ) -> Result<(DocumentStateSnapshot, DocumentEventSubscription), ControllerError> {
        let controller = self.lock()?;
        let snapshot = controller.state_snapshot();
        let next_sequence = snapshot.sequence.saturating_add(1);
        Ok((
            snapshot,
            DocumentEventSubscription {
                handle: self.clone(),
                next_sequence,
            },
        ))
    }
}

impl DocumentEventSubscription {
    /// 按 sequence 读取保留事件；日志已回收时返回 typed lag，调用方需重新订阅。
    pub fn poll(&mut self) -> Result<Vec<DocumentEvent>, ControllerError> {
        let controller = self.handle.lock()?;
        let oldest = controller.events.front().map(DocumentEvent::sequence);
        if let Some(oldest) = oldest
            && self.next_sequence < oldest
        {
            return Err(ControllerError::SubscriptionLagged {
                expected: self.next_sequence,
                oldest,
            });
        }
        let events = controller
            .events
            .iter()
            .filter(|event| event.sequence() >= self.next_sequence)
            .cloned()
            .collect::<Vec<_>>();
        if let Some(last) = events.last() {
            self.next_sequence = last.sequence().saturating_add(1);
        }
        Ok(events)
    }

    /// 只探测游标后是否已有事件，不推进游标，避免空闲视图被无效重绘。
    pub fn has_pending(&self) -> Result<bool, ControllerError> {
        let controller = self.handle.lock()?;
        let oldest = controller.events.front().map(DocumentEvent::sequence);
        if let Some(oldest) = oldest
            && self.next_sequence < oldest
        {
            return Err(ControllerError::SubscriptionLagged {
                expected: self.next_sequence,
                oldest,
            });
        }
        Ok(controller
            .events
            .back()
            .is_some_and(|event| event.sequence() >= self.next_sequence))
    }
}
