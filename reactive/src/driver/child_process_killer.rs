struct ChildProcessKillerDriver(mpsc::Sender<Result<(), std::io::Error>>);

impl Driver for ChildProcessKillerDriver {
    type Init = ();
    type Input = LocalBoxStream<'static, Child>;
    type Output = LocalBoxStream<'static, Result<(), std::io::Error>>;

    fn new(_init: Self::Init) -> (Self, Self::Output) {
        let (sender, mut receiver) = mpsc::channel(1);

        let stream = fn_stream(|emitter| async move {
            loop {
                let value = receiver.recv().await.expect(NEVER_ENDING_STREAM);
                emitter.emit(value).await;
            }
        })
        .boxed();

        let child_killer_driver = Self(sender);

        (child_killer_driver, stream)
    }

    fn init(self, mut kill_child: Self::Input) -> LocalBoxFuture<'static, ()> {
        async move {
            loop {
                let mut child = kill_child.next().await.expect(NEVER_ENDING_STREAM);
                let result = child.kill().await;
                self.0.send(result).await.unwrap();
            }
        }
        .boxed_local()
    }
}
