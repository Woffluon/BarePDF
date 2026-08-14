use crate::protocol::RenderCommand;
use crossbeam_channel::{Receiver, Select, TryRecvError};

const VISIBLE_BURST: usize = 8;

pub(crate) fn receive_command(
    shutdown_receiver: &Receiver<()>,
    control_receiver: &Receiver<RenderCommand>,
    visible_receiver: &Receiver<RenderCommand>,
    low_receiver: &Receiver<RenderCommand>,
    visible_budget: &mut usize,
) -> Option<RenderCommand> {
    loop {
        match shutdown_receiver.try_recv() {
            Ok(()) | Err(TryRecvError::Disconnected) => return None,
            Err(TryRecvError::Empty) => {}
        }

        let control_open = match control_receiver.try_recv() {
            Ok(command) => return Some(command),
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => false,
        };

        let mut low_open = true;
        if *visible_budget >= VISIBLE_BURST {
            low_open = match low_receiver.try_recv() {
                Ok(command) => {
                    *visible_budget = 0;
                    return Some(command);
                }
                Err(TryRecvError::Empty) => true,
                Err(TryRecvError::Disconnected) => false,
            };
        }

        let visible_open = match visible_receiver.try_recv() {
            Ok(command) => {
                *visible_budget = visible_budget.saturating_add(1);
                return Some(command);
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => false,
        };

        if *visible_budget < VISIBLE_BURST {
            low_open = match low_receiver.try_recv() {
                Ok(command) => {
                    *visible_budget = 0;
                    return Some(command);
                }
                Err(TryRecvError::Empty) => true,
                Err(TryRecvError::Disconnected) => false,
            };
        }

        if !control_open && !visible_open && !low_open {
            return None;
        }

        let mut ready = Select::new();
        ready.recv(shutdown_receiver);
        if control_open {
            ready.recv(control_receiver);
        }
        if visible_open {
            ready.recv(visible_receiver);
        }
        if low_open {
            ready.recv(low_receiver);
        }
        let _ = ready.ready();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use barepdf_core::DocumentId;
    use crossbeam_channel::bounded;

    #[test]
    fn control_then_visible_then_low_are_strictly_prioritized() {
        let (_shutdown_sender, shutdown_receiver) = bounded(1);
        let (control_sender, control_receiver) = bounded(1);
        let (visible_sender, visible_receiver) = bounded(1);
        let (low_sender, low_receiver) = bounded(1);
        control_sender
            .try_send(RenderCommand::CloseDocument(DocumentId::new(1)))
            .expect("control queue has one free slot");
        visible_sender
            .try_send(RenderCommand::CloseDocument(DocumentId::new(2)))
            .expect("visible queue has one free slot");
        low_sender
            .try_send(RenderCommand::CloseDocument(DocumentId::new(3)))
            .expect("low queue has one free slot");
        let mut visible_budget = 0;

        assert!(matches!(
            receive_command(
                &shutdown_receiver,
                &control_receiver,
                &visible_receiver,
                &low_receiver,
                &mut visible_budget,
            ),
            Some(RenderCommand::CloseDocument(id)) if id == DocumentId::new(1)
        ));
        assert!(matches!(
            receive_command(
                &shutdown_receiver,
                &control_receiver,
                &visible_receiver,
                &low_receiver,
                &mut visible_budget,
            ),
            Some(RenderCommand::CloseDocument(id)) if id == DocumentId::new(2)
        ));
        assert!(matches!(
            receive_command(
                &shutdown_receiver,
                &control_receiver,
                &visible_receiver,
                &low_receiver,
                &mut visible_budget,
            ),
            Some(RenderCommand::CloseDocument(id)) if id == DocumentId::new(3)
        ));
    }

    #[test]
    fn low_work_runs_after_eight_visible_commands() {
        let (_shutdown_sender, shutdown_receiver) = bounded(1);
        let (control_sender, control_receiver) = bounded(1);
        let (visible_sender, visible_receiver) = bounded(9);
        let (low_sender, low_receiver) = bounded(1);
        for id in 0..9 {
            visible_sender
                .try_send(RenderCommand::CloseDocument(DocumentId::new(id)))
                .expect("visible queue has room for test commands");
        }
        low_sender
            .try_send(RenderCommand::CloseDocument(DocumentId::new(99)))
            .expect("low queue has one free slot");
        let mut visible_budget = 0;

        for expected in 0..VISIBLE_BURST {
            assert!(matches!(
                receive_command(
                    &shutdown_receiver,
                    &control_receiver,
                    &visible_receiver,
                    &low_receiver,
                    &mut visible_budget,
                ),
                Some(RenderCommand::CloseDocument(id)) if id == DocumentId::new(expected as u64)
            ));
        }
        control_sender
            .try_send(RenderCommand::CloseDocument(DocumentId::new(100)))
            .expect("control queue has one free slot");
        assert!(matches!(
            receive_command(
                &shutdown_receiver,
                &control_receiver,
                &visible_receiver,
                &low_receiver,
                &mut visible_budget,
            ),
            Some(RenderCommand::CloseDocument(id)) if id == DocumentId::new(100)
        ));
        assert!(matches!(
            receive_command(
                &shutdown_receiver,
                &control_receiver,
                &visible_receiver,
                &low_receiver,
                &mut visible_budget,
            ),
            Some(RenderCommand::CloseDocument(id)) if id == DocumentId::new(99)
        ));
    }
}
