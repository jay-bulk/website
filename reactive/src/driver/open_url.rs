struct OpenUrlDriver(CompleteHandle<Result<(), std::io::Error>>);

impl Driver for OpenUrlDriver {
    type Init = ();
    type Input = LocalBoxFuture<'static, Url>;
    type Output = LocalBoxFuture<'static, Result<(), std::io::Error>>;
    fn new(_init: Self::Init) -> (Self, Self::Output) {
        let (future, handle) = future_handles::sync::create();
        (Self(handle), future.map(Result::unwrap).boxed_local())
    }

    fn init(self, url: Self::Input) -> LocalBoxFuture<'static, ()> {
        async move {
            self.0.complete(open::that(url.await.as_str()));
            future::pending::<()>().await;
        }
        .boxed_local()
    }
}

