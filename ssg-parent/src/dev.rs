mod app;

use futures::FutureExt;
use reactive::driver::Driver;

#[derive(Debug, thiserror::Error)]
#[allow(clippy::module_name_repetitions)]
pub enum DevError {
    #[error(transparent)]
    FsWatch(reactive::driver::fs_change::Error),
    #[error(transparent)]
    Io(std::io::Error),
    #[error("no free port")]
    NoFreePort,
}

const BUILDER_CRATE_NAME: &str = "builder";
const LOCALHOST: &str = "localhost";

#[allow(clippy::missing_panics_doc)]
pub async fn dev<O: AsRef<camino::Utf8Path>>(launch_browser: bool, output_dir: O) -> DevError {
    let output_dir = output_dir.as_ref().to_owned();
    let Some(port) = portpicker::pick_unused_port() else {
        return DevError::NoFreePort
    };

    let server_task = live_server::listen(LOCALHOST, port, output_dir.as_std_path().to_owned())
        .map(|result| result.expect_err("unreachable"))
        .boxed();

    let mut cargo_run_builder = tokio::process::Command::new("cargo");
    cargo_run_builder.args(["run", "--package", BUILDER_CRATE_NAME]);

    let url = reqwest::Url::parse(&format!("http://{LOCALHOST}:{port}")).unwrap();

    let (builder_driver, builder_started) =
        reactive::driver::command::StaticCommandDriver::new(cargo_run_builder);
    let (child_process_killer_driver, child_killed) =
        reactive::driver::child_process_killer::ChildProcessKillerDriver::new(());
    let (open_url_driver, browser_launch) =
        reactive::driver::open_that::StaticOpenThatDriver::new(url.to_string());
    let (eprintln_driver, _) = reactive::driver::eprintln::EprintlnDriver::new(());
    let (fs_change_driver, fs_change) =
        reactive::driver::fs_change::FsChangeDriver::new(BUILDER_CRATE_NAME);

    let inputs = app::Inputs {
        server_task,
        child_killed,
        fs_change,
        builder_started,
        launch_browser,
        open_that_driver: browser_launch,
        local_host_port_url: url,
    };

    let outputs = app::app(inputs);

    let app::Outputs {
        stderr,
        open_browser: launch_browser,
        error: app_error,
        kill_child,
        run_builder,
        some_task,
    } = outputs;

    let builder_driver_task = builder_driver.init(run_builder);
    let child_killer_task = child_process_killer_driver.init(kill_child);
    let open_url_driver_task = open_url_driver.init(launch_browser);
    let stderr_driver_task = eprintln_driver.init(stderr);
    let fs_change_driver_task = fs_change_driver.init(());

    futures::select! {
        error = app_error.fuse() => error,
        _ = builder_driver_task.fuse() => unreachable!(),
        _ = child_killer_task.fuse() => unreachable!(),
        _ = stderr_driver_task.fuse() => unreachable!(),
        _ = open_url_driver_task.fuse() => unreachable!(),
        _ = some_task.fuse() => unreachable!(),
        _ = fs_change_driver_task.fuse() => unreachable!(),
    }
}
