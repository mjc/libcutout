//! Foreground monitor intent independent of provider SDK effects.

/// Matches a provider callback path, treating only empty and slash roots as equal.
/// Scheme and host matching remain the platform URL parser's responsibility.
#[must_use]
pub fn music_callback_path_matches(expected: &str, actual: &str) -> bool {
    match (expected, actual) {
        ("" | "/", "" | "/") => true,
        _ => expected == actual,
    }
}

/// What a foreground provider monitor may do when it starts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MusicMonitorStart {
    /// Observe or reconnect using existing authorization without launching auth UI.
    Observe,
    /// Consume one explicit user request to launch authorization if necessary.
    Authorize,
}

/// User intent for the next foreground monitor start.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MusicMonitorRequest {
    /// Observe or reconnect using existing authorization.
    Observe,
    /// Consume one explicit user request to launch authorization if necessary.
    Authorize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MonitorScene {
    Active,
    Suspended,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MonitorIntent {
    Idle,
    Observe,
    Authorize,
}

/// Result of bringing a monitor back to the foreground.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MusicMonitorResume {
    /// The monitor was already in the foreground.
    AlreadyActive,
    /// No monitor request was waiting when the scene resumed.
    NoRequest,
    /// A requested monitor was restored after suspension.
    Restored,
}

/// Foreground music intent, independent of ride recording and history retention.
///
/// Platform code owns the SDK task and credential callbacks. A callback can save
/// credentials in either scene state; it never grants another authorization
/// launch. Restored preferences request passive observation only.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MusicMonitor {
    scene: MonitorScene,
    intent: MonitorIntent,
}

impl Default for MusicMonitor {
    fn default() -> Self {
        Self {
            scene: MonitorScene::Active,
            intent: MonitorIntent::Idle,
        }
    }
}

impl MusicMonitor {
    /// Requests observation; only an explicit user action may allow authorization.
    pub fn request(&mut self, request: MusicMonitorRequest) {
        self.intent = match request {
            MusicMonitorRequest::Observe => match self.intent {
                MonitorIntent::Authorize => MonitorIntent::Authorize,
                MonitorIntent::Idle | MonitorIntent::Observe => MonitorIntent::Observe,
            },
            MusicMonitorRequest::Authorize => MonitorIntent::Authorize,
        };
    }

    /// Cancels observation and any unconsumed authorization request.
    pub fn cancel(&mut self) {
        self.intent = MonitorIntent::Idle;
    }

    /// Suspends observation without forgetting intent or retaining an auth launch.
    pub fn suspend(&mut self) {
        self.scene = MonitorScene::Suspended;
        if self.intent == MonitorIntent::Authorize {
            self.intent = MonitorIntent::Observe;
        }
    }

    /// Returns the foreground transition outcome and restores active observation.
    #[must_use]
    pub fn resume(&mut self) -> MusicMonitorResume {
        let result = match self.scene {
            MonitorScene::Active => MusicMonitorResume::AlreadyActive,
            MonitorScene::Suspended => match self.intent {
                MonitorIntent::Idle => MusicMonitorResume::NoRequest,
                MonitorIntent::Observe | MonitorIntent::Authorize => MusicMonitorResume::Restored,
            },
        };
        self.scene = MonitorScene::Active;
        result
    }

    /// Whether the platform scene permits foreground observation.
    #[must_use]
    pub const fn is_scene_active(&self) -> bool {
        match self.scene {
            MonitorScene::Active => true,
            MonitorScene::Suspended => false,
        }
    }

    /// Returns the start that would be admitted without consuming the intent.
    #[must_use]
    pub fn pending_start(&self) -> Option<MusicMonitorStart> {
        if self.scene != MonitorScene::Active || self.intent == MonitorIntent::Idle {
            return None;
        }
        Some(match self.intent {
            MonitorIntent::Idle | MonitorIntent::Observe => MusicMonitorStart::Observe,
            MonitorIntent::Authorize => MusicMonitorStart::Authorize,
        })
    }

    /// Admits a foreground start and consumes any one-shot authorization grant.
    ///
    /// Further starts remain passive until another explicit request. The platform
    /// must cancel or replace its existing SDK task before starting another one.
    #[must_use]
    pub fn take_start(&mut self) -> Option<MusicMonitorStart> {
        let start = self.pending_start()?;
        self.intent = MonitorIntent::Observe;
        Some(start)
    }
}
