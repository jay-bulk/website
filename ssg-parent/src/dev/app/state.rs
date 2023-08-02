#[derive(Debug, Default)]
pub(super) struct State {
    builder: BuilderState,
}
impl State {
    pub(super) fn input_event(
        &mut self,
        input: super::InputEvent,
    ) -> Option<super::OutputEvent> {
        match input {
            super::InputEvent::BuilderKilled(result) => self.builder_killed(result),
            super::InputEvent::FsChange(result) => self.fs_change(result),
            super::InputEvent::BuilderStarted(child) => self.builder_started(child),
            super::InputEvent::BrowserLaunched(result) => match result {
                Ok(_) => None,
                Err(error) => {
                    Some(super::OutputEvent::Error(super::super::DevError::Io(error)))
                }
            },
            super::InputEvent::ServerError(error) => {
                Some(super::OutputEvent::Error(super::super::DevError::Io(error)))
            }
        }
    }

    #[allow(clippy::unnecessary_wraps)]
    fn builder_killed(
        &mut self,
        result: Result<(), std::io::Error>,
    ) -> Option<super::OutputEvent> {
        match result {
            Ok(_) => {
                self.builder = BuilderState::None;
                Some(super::OutputEvent::RunBuilder)
            }
            Err(error) => Some(super::OutputEvent::Error(super::super::DevError::Io(error))),
        }
    }

    fn fs_change(
        &mut self,
        result: Result<reactive::driver::fs_change::Event, reactive::driver::fs_change::Error>,
    ) -> Option<super::OutputEvent> {
        match result {
            Ok(event) => match event.kind {
                reactive::driver::fs_change::EventKind::Create(_)
                | reactive::driver::fs_change::EventKind::Modify(_)
                | reactive::driver::fs_change::EventKind::Remove(_) => {
                    match &mut self.builder {
                        BuilderState::Starting => {
                            self.builder = BuilderState::Obsolete;
                            None
                        }
                        BuilderState::Started(_) => {
                            let child = self.builder.killing().unwrap();
                            Some(super::OutputEvent::KillChild(child))
                        }
                        BuilderState::None | BuilderState::Obsolete => None,
                    }
                }
                _ => None,
            },
            Err(error) => Some(super::OutputEvent::Error(super::super::DevError::FsWatch(
                error,
            ))),
        }
    }

    fn builder_started(
        &mut self,
        child: Result<tokio::process::Child, std::io::Error>,
    ) -> Option<super::OutputEvent> {
        match child {
            Ok(child) => match self.builder {
                BuilderState::None | BuilderState::Starting => {
                    self.builder = BuilderState::Started(child);
                    None
                }
                BuilderState::Obsolete => {
                    self.builder = BuilderState::None;
                    Some(super::OutputEvent::KillChild(child))
                }
                BuilderState::Started(_) => {
                    // TODO is this reachable?
                    let current_child = self.builder.killing().unwrap();
                    self.builder = BuilderState::Started(child);
                    Some(super::OutputEvent::KillChild(current_child))
                }
            },
            Err(error) => Some(super::OutputEvent::Error(super::super::DevError::Io(error))),
        }
    }
}

#[derive(Debug, Default)]
enum BuilderState {
    None,
    Obsolete,
    #[default]
    Starting,
    Started(tokio::process::Child),
}

impl BuilderState {
    fn killing(&mut self) -> Option<tokio::process::Child> {
        if let Self::Started(child) = std::mem::replace(self, Self::None) {
            Some(child)
        } else {
            None
        }
    }
}

