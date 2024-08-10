use tokio::{
    select,
    signal::unix::{signal, Signal, SignalKind},
};
use tracing::info;

pub struct SignalHandler {
    terminate: Signal,
    interrupt: Signal,
    quit: Signal,
    hangup: Signal,
    handler_id: String,
}

impl SignalHandler {
    pub fn new(handler_id: impl Into<String>) -> Self {
        Self {
            handler_id: handler_id.into(),
            terminate: signal(SignalKind::terminate())
                .expect("{listener_name}: unable to install TERM signal handler"),
            interrupt: signal(SignalKind::interrupt())
                .expect("{listener_name}: unable to install INT signal handler"),
            quit: signal(SignalKind::quit())
                .expect("{listener_name}: unable to install QUIT signal handler"),
            hangup: signal(SignalKind::hangup())
                .expect("{listener_name}: unable to install HANGUP signal handler"),
        }
    }

    pub async fn wait(&mut self) {
        let sig = select! {
            _ = self.terminate.recv() => "TERM",
            _ = self.interrupt.recv() => "INT",
            _ = self.quit.recv() => "QUIT",
            _ = self.hangup.recv() => "HANGUP",
        };

        info!(handler_id = %self.handler_id, signal = %sig,
            "signal has been received, shutting down",
        );
    }
}
