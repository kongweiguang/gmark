// @author kongweiguang

//! Update state machine and command policy.

use super::*;

#[derive(Clone, Debug, Default)]

pub(crate) enum UpdateState {
    #[default]
    Idle,
    /// The cache is inspected off the UI thread so a large package cannot delay startup.
    Restoring,
    Checking {
        origin: CheckOrigin,
    },
    UpToDate {
        current_version: String,
        latest_version: String,
    },
    Available(UpdateRelease),
    Downloading {
        release: UpdateRelease,
        downloaded: u64,
        total: u64,
        bytes_per_second: u64,
    },
    Paused {
        release: UpdateRelease,
        downloaded: u64,
        total: u64,
    },
    Verifying {
        release: UpdateRelease,
    },
    Ready {
        release: UpdateRelease,
        artifact_path: PathBuf,
    },
    /// Velopack is importing the verified package into its own cache off the UI thread.
    StagingInstall {
        release: UpdateRelease,
        artifact_path: PathBuf,
    },
    /// The package is locally staged; only the ordinary quit approval may hand it off.
    Staged {
        release: UpdateRelease,
        artifact_path: PathBuf,
    },
    Succeeded {
        version: String,
        message: String,
    },
    Failed {
        release: Option<UpdateRelease>,
        message: String,
        retryable: bool,
    },
}

impl UpdateState {
    pub(crate) fn is_visible(&self) -> bool {
        !matches!(self, Self::Idle)
            && !matches!(
                self,
                Self::Checking {
                    origin: CheckOrigin::Automatic
                }
            )
    }

    pub(crate) fn release(&self) -> Option<&UpdateRelease> {
        match self {
            Self::Available(release)
            | Self::Downloading { release, .. }
            | Self::Paused { release, .. }
            | Self::Verifying { release }
            | Self::Ready { release, .. }
            | Self::StagingInstall { release, .. }
            | Self::Staged { release, .. } => Some(release),
            Self::Failed { release, .. } => release.as_ref(),
            Self::Idle
            | Self::Restoring
            | Self::Checking { .. }
            | Self::UpToDate { .. }
            | Self::Succeeded { .. } => None,
        }
    }

    /// 命令准入是状态机的纯决策层；UI 与后台事件都不能绕过同一组幂等边界。
    pub(super) fn accepts(&self, command: UpdateCommand) -> bool {
        match command {
            UpdateCommand::Check => !matches!(
                self,
                Self::Restoring
                    | Self::Checking { .. }
                    | Self::Downloading { .. }
                    | Self::Verifying { .. }
                    | Self::StagingInstall { .. }
                    | Self::Staged { .. }
            ),
            UpdateCommand::Download => matches!(self, Self::Available(_)),
            UpdateCommand::Pause => matches!(self, Self::Downloading { .. }),
            UpdateCommand::Resume => matches!(self, Self::Paused { .. }),
            UpdateCommand::Retry => matches!(
                self,
                Self::Failed {
                    retryable: true,
                    ..
                }
            ),
            UpdateCommand::InstallAndRestart => {
                matches!(self, Self::Ready { .. } | Self::Staged { .. })
            }
            UpdateCommand::Dismiss => !matches!(
                self,
                Self::Restoring
                    | Self::Downloading { .. }
                    | Self::Verifying { .. }
                    | Self::StagingInstall { .. }
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UpdateCommand {
    Check,
    Download,
    Pause,
    Resume,
    Retry,
    InstallAndRestart,
    Dismiss,
}
