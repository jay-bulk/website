mod state;

use colored::Colorize;
use futures::{FutureExt, SinkExt, StreamExt};

enum InputEvent {
    BuilderKilled(Result<(), std::io::Error>),
    Notify(reactive::driver::notify::Result<reactive::driver::notify::Event>),
    BuilderStarted(Result<tokio::process::Child, std::io::Error>),
    BrowserOpened(Result<(), std::io::Error>),
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

pub(super) struct Inputs {
    pub(super) server_task: futures::future::LocalBoxFuture<'static, std::io::Error>,
    pub(super) child_killed: futures::stream::LocalBoxStream<'static, Result<(), std::io::Error>>,
    pub(super) notify: futures::stream::LocalBoxStream<
        'static,
        reactive::driver::notify::Result<reactive::driver::notify::Event>,
    >,
    pub(super) builder_started:
        futures::stream::LocalBoxStream<'static, Result<tokio::process::Child, std::io::Error>>,
    pub(super) launch_browser: bool,
    pub(super) browser_opened: futures::stream::LocalBoxStream<'static, Result<(), std::io::Error>>,
    pub(super) url: reqwest::Url,
}

pub(super) struct Outputs {
    pub(super) stderr: futures::stream::LocalBoxStream<'static, String>,
    pub(super) kill_child: futures::stream::LocalBoxStream<'static, tokio::process::Child>,
    pub(super) run_builder: futures::stream::LocalBoxStream<'static, ()>,
    pub(super) open_browser: futures::stream::LocalBoxStream<'static, ()>,
    pub(super) error: futures::future::LocalBoxFuture<'static, super::DevError>,
    pub(super) stream_splitter_task: futures::future::LocalBoxFuture<'static, ()>,
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
    let initial = futures::stream::iter(initial);

    let reaction = futures::stream::select_all([
        futures::stream::once(server_task)
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
        futures::future::ready(Some(state.input_event(input)))
    })
    .filter_map(futures::future::ready);

    let mut output = initial.chain(reaction);

    let (mut kill_child_sender, kill_child) = futures::channel::mpsc::channel(1);
    let (mut run_builder_sender, run_builder) = futures::channel::mpsc::channel(1);
    let (mut error_sender, error) = futures::channel::mpsc::channel(1);
    let (mut stderr_sender, stderr) = futures::channel::mpsc::channel(1);
    let (mut open_browser_sender, open_browser) = futures::channel::mpsc::channel(1);

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
