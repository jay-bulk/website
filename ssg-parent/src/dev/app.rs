mod state{

#[derive(Debug, Default)]
struct State {
    builder: BuilderState,
}
impl State {
    fn input_event(&mut self, input: super::InputEvent) -> Option<super::OutputEvent> {
        match input {
            super::InputEvent::BuilderKilled(result) => self.builder_killed(result),
            super::InputEvent::FsChange(result) => self.fs_change(result),
            super::InputEvent::BuilderStarted(child) => self.builder_started(child),
            super::InputEvent::BrowserLaunched(result) => match result {
                Ok(_) => None,
                Err(error) => Some(super::OutputEvent::Error(super::super::DevError::Io(error))),
            },
            InputEvent::ServerError(error) => Some(OutputEvent::Error(super::DevError::Io(error))),
        }
    }

    #[allow(clippy::unnecessary_wraps)]
    fn builder_killed(&mut self, result: Result<(), std::io::Error>) -> Option<OutputEvent> {
        match result {
            Ok(_) => {
                self.builder = BuilderState::None;
                Some(OutputEvent::RunBuilder)
            }
            Err(error) => Some(OutputEvent::Error(super::DevError::Io(error))),
        }
    }

    fn fs_change(
        &mut self,
        result: Result<reactive::driver::fs_change::Event, reactive::driver::fs_change::Error>,
    ) -> Option<OutputEvent> {
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
                            Some(OutputEvent::KillChild(child))
                        }
                        BuilderState::None | BuilderState::Obsolete => None,
                    }
                }
                _ => None,
            },
            Err(error) => Some(OutputEvent::Error(super::DevError::FsWatch(error))),
        }
    }

    fn builder_started(
        &mut self,
        child: Result<tokio::process::Child, std::io::Error>,
    ) -> Option<OutputEvent> {
        match child {
            Ok(child) => match self.builder {
                BuilderState::None | BuilderState::Starting => {
                    self.builder = BuilderState::Started(child);
                    None
                }
                BuilderState::Obsolete => {
                    self.builder = BuilderState::None;
                    Some(OutputEvent::KillChild(child))
                }
                BuilderState::Started(_) => {
                    // TODO is this reachable?
                    let current_child = self.builder.killing().unwrap();
                    self.builder = BuilderState::Started(child);
                    Some(OutputEvent::KillChild(current_child))
                }
            },
            Err(error) => Some(OutputEvent::Error(super::DevError::Io(error))),
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

}
use colored::Colorize;
use futures::{SinkExt, FutureExt, StreamExt};

enum InputEvent {
    BuilderKilled(Result<(), std::io::Error>),
    FsChange(reactive::driver::fs_change::Result<reactive::driver::fs_change::Event>),
    BuilderStarted(Result<tokio::process::Child, std::io::Error>),
    BrowserLaunched(Result<(), std::io::Error>),
    ServerError(std::io::Error),
}

#[derive(Debug)]
enum OutputEvent {
    Stderr(String),
    RunBuilder,
    KillChild(tokio::process::Child),
    Error(super::DevError),
    OpenBrowser,
}


fn send_event_value<T: 'static>(
    sender: &futures::channel::mpsc::Sender<T>,
    value: T,
) -> std::pin::Pin<Box<dyn futures::Future<Output = ()>>> {
    let mut sender_clone = sender.clone();
    async move {
        sender_clone.send(value).await.unwrap();
    }
    .boxed_local()
}

pub(super) struct Inputs {
    pub(super) server_task: futures::future::LocalBoxFuture<'static, std::io::Error>,
    pub(super) child_killed: futures::stream::LocalBoxStream<'static, Result<(), std::io::Error>>,
    pub(super) fs_change: futures::stream::LocalBoxStream<
        'static,
        reactive::driver::fs_change::Result<reactive::driver::fs_change::Event>,
    >,
    pub(super) builder_started:
        futures::stream::LocalBoxStream<'static, Result<tokio::process::Child, std::io::Error>>,
    pub(super) launch_browser: bool,
    pub(super) open_that_driver: futures::stream::LocalBoxStream<'static, Result<(), std::io::Error>>,
    pub(super) local_host_port_url: reqwest::Url,
}

pub(super) struct Outputs {
    pub(super) stderr: futures::stream::LocalBoxStream<'static, String>,
    pub(super) kill_child: futures::stream::LocalBoxStream<'static, tokio::process::Child>,
    pub(super) run_builder: futures::stream::LocalBoxStream<'static, ()>,
    pub(super) open_browser: futures::stream::LocalBoxStream<'static, ()>,
    pub(super) error: futures::future::LocalBoxFuture<'static, super::DevError>,
    pub(super) some_task: futures::future::LocalBoxFuture<'static, ()>,
}

pub(super) fn app(inputs: Inputs) -> Outputs {
    let Inputs {
        server_task,
        child_killed,
        fs_change: builder_crate_fs_change,
        builder_started,
        launch_browser,
        open_that_driver: browser_launch,
        local_host_port_url,
    } = inputs;

    let message = format!("\nServer started at {local_host_port_url}\n")
        .blue()
        .to_string();

    let mut initial = vec![OutputEvent::RunBuilder, OutputEvent::Stderr(message)];
    if launch_browser {
        initial.push(OutputEvent::OpenBrowser);
    }
    let initial = futures::stream::iter(initial);

    let reaction = futures::stream::select_all([
        futures::stream::once(server_task)
            .map(InputEvent::ServerError)
            .boxed_local(),
        child_killed.map(InputEvent::BuilderKilled).boxed_local(),
        builder_crate_fs_change
            .map(InputEvent::FsChange)
            .boxed_local(),
        builder_started
            .map(InputEvent::BuilderStarted)
            .boxed_local(),
        browser_launch
            .map(InputEvent::BrowserLaunched)
            .boxed_local(),
    ])
    .scan(State::default(), move |state, input| {
        futures::future::ready(Some(state.input_event(input)))
    })
    .filter_map(futures::future::ready);

    let output = initial.chain(reaction);

    let (kill_child_sender, kill_child) = futures::channel::mpsc::channel(1);
    let (run_builder_sender, run_builder) = futures::channel::mpsc::channel(1);
    let (error_sender, error) = futures::channel::mpsc::channel(1);
    let (stderr_sender, stderr) = futures::channel::mpsc::channel(1);
    let (open_browser_sender, open_browser) = futures::channel::mpsc::channel(1);

    let some_task = output
        .for_each(move |event| match event {
            OutputEvent::RunBuilder => send_event_value(&run_builder_sender, ()),
            OutputEvent::KillChild(child) => send_event_value(&kill_child_sender, child),
            OutputEvent::Error(error) => send_event_value(&error_sender, error),
            OutputEvent::Stderr(output) => send_event_value(&stderr_sender, output),
            OutputEvent::OpenBrowser => send_event_value(&open_browser_sender, ()),
        })
        .boxed_local();

    let error = error
        .into_future()
        .map(|(error, _tail_of_stream)| error.unwrap())
        .boxed_local();

    Outputs {
        stderr: stderr.boxed_local(),
        kill_child: kill_child.boxed_local(),
        run_builder: run_builder.boxed_local(),
        open_browser: open_browser.boxed_local(),
        error,
        some_task,
    }
}
