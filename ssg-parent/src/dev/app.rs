mod state;

use colored::Colorize;
use futures::{
    channel::mpsc,
    future::{self, LocalBoxFuture},
    stream::{self, LocalBoxStream},
    FutureExt, SinkExt, StreamExt,
};

#[derive(Debug)]
enum InputEvent {
    BuilderKilled(Result<(), std::io::Error>),
    Notify(reactive::driver::notify::Result<reactive::driver::notify::Event>),
    BuilderStarted(std::io::Result<tokio::process::Child>),
    BrowserOpened(std::io::Result<()>),
    ServerError(std::io::Error),
}

#[derive(Debug)]
enum OutputEvent {
    Stderr(String),
    RunBuilder,
    KillChildProcess(tokio::process::Child),
    Error(super::DevError),
    OpenBrowser,
}

/// Inputs for bootstrapping the reactive app
pub(super) struct Inputs {
    pub(super) server_task: LocalBoxFuture<'static, std::io::Error>,
    pub(super) child_killed: LocalBoxStream<'static, std::io::Result<()>>,
    pub(super) notify:
        LocalBoxStream<'static, reactive::driver::notify::Result<reactive::driver::notify::Event>>,
    pub(super) builder_started: LocalBoxStream<'static, std::io::Result<tokio::process::Child>>,
    pub(super) launch_browser: bool,
    pub(super) browser_opened: LocalBoxStream<'static, std::io::Result<()>>,
    pub(super) url: reqwest::Url,
}

/// Outputs from the reactive app
pub(super) struct Outputs {
    pub(super) stderr: LocalBoxStream<'static, String>,
    pub(super) kill_child: LocalBoxStream<'static, tokio::process::Child>,
    pub(super) run_builder: LocalBoxStream<'static, ()>,
    pub(super) open_browser: LocalBoxStream<'static, ()>,
    pub(super) error: LocalBoxFuture<'static, super::DevError>,
    pub(super) stream_splitter_task: LocalBoxFuture<'static, ()>,
}

/// Initializes the state machine for the dev environment
pub(super) fn app(inputs: Inputs) -> Outputs {
    let Inputs {
        server_task,
        child_killed,
        notify: builder_crate_fs_change,
        builder_started,
        launch_browser,
        browser_opened: browser_launch,
        url: local_host_port_url,
    } = inputs;

    let message = format!("\nServer started at {local_host_port_url}\n")
        .blue()
        .to_string();

    let mut initial = vec![OutputEvent::RunBuilder, OutputEvent::Stderr(message)];
    if launch_browser {
        initial.push(OutputEvent::OpenBrowser);
    }
    let initial = stream::iter(initial);

    let reaction = stream::select_all([
        stream::once(server_task)
            .map(InputEvent::ServerError)
            .boxed_local(),
        child_killed.map(InputEvent::BuilderKilled).boxed_local(),
        builder_crate_fs_change
            .map(InputEvent::Notify)
            .boxed_local(),
        builder_started
            .map(InputEvent::BuilderStarted)
            .boxed_local(),
        browser_launch.map(InputEvent::BrowserOpened).boxed_local(),
    ])
    .scan(state::State::default(), move |state, input| {
        future::ready(Some(state.input_event(input)))
    })
    .filter_map(future::ready);

    let mut output = initial.chain(reaction);

    let (mut kill_child_sender, kill_child) = mpsc::channel(1);
    let (mut run_builder_sender, run_builder) = mpsc::channel(1);
    let (mut error_sender, error) = mpsc::channel(1);
    let (mut stderr_sender, stderr) = mpsc::channel(1);
    let (mut open_browser_sender, open_browser) = mpsc::channel(1);

    let stream_splitter_task = async move {
        loop {
            let event = output.next().await.unwrap();
            match event {
                OutputEvent::Stderr(output) => {
                    stderr_sender.send(output).await.unwrap();
                }
                OutputEvent::RunBuilder => {
                    run_builder_sender.send(()).await.unwrap();
                }
                OutputEvent::KillChildProcess(child) => {
                    kill_child_sender.send(child).await.unwrap();
                }
                OutputEvent::Error(error) => {
                    error_sender.send(error).await.unwrap();
                }
                OutputEvent::OpenBrowser => {
                    open_browser_sender.send(()).await.unwrap();
                }
            }
        }
    }
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
        stream_splitter_task,
    }
}
