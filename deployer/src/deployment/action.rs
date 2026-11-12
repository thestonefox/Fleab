//! A deployment and its optional launch share one captured profile and build.
use super::{Installed, Progress};
use crate::{
    archives::Build,
    launching::{self, Runner},
    profiles::{Config, Profile},
};
use anyhow::Result;
use std::{
    process::Child,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Debug)]
pub enum Launch<C = Child> {
    NotRequested,
    Cancelled,
    Started(C),
    Failed(anyhow::Error),
}

pub struct Completion {
    pub installed: Installed,
    pub launch: Launch,
}

pub fn run(
    config: &Config,
    profile: &Profile,
    build: &Build,
    launch_after: bool,
    runners: &[Runner],
    cancel: Arc<AtomicBool>,
    mut progress: impl FnMut(Progress),
) -> Result<Completion> {
    if launch_after {
        launching::check_build(profile, build, std::env::consts::OS, runners)?;
    }
    let (installed, launch) = sequence(
        &cancel,
        || super::deploy(config, profile, build, cancel.clone(), &mut progress),
        launch_after.then_some(|| launching::launch(profile, build.target, runners)),
    )?;
    Ok(Completion { installed, launch })
}

// Keep the committed installation even if launch fails. Cancellation is checked
// again after deployment because committing a replacement cannot be interrupted.
fn sequence<I, C>(
    cancel: &AtomicBool,
    deploy: impl FnOnce() -> Result<I>,
    launch: Option<impl FnOnce() -> Result<C>>,
) -> Result<(I, Launch<C>)> {
    let installed = deploy()?;
    let launched = match launch {
        None => Launch::NotRequested,
        Some(_) if cancel.load(Ordering::Relaxed) => Launch::Cancelled,
        Some(launch) => match launch() {
            Ok(child) => Launch::Started(child),
            Err(error) => Launch::Failed(error),
        },
    };
    Ok((installed, launched))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn launch_follows_successful_deployment() {
        let events = RefCell::new(Vec::new());
        let (installed, launched) = sequence(
            &AtomicBool::new(false),
            || {
                events.borrow_mut().push("deploy");
                Ok("new installation")
            },
            Some(|| {
                events.borrow_mut().push("launch");
                Ok("child")
            }),
        )
        .unwrap();
        assert_eq!(*events.borrow(), ["deploy", "launch"]);
        assert_eq!(installed, "new installation");
        assert!(matches!(launched, Launch::Started("child")));
    }

    #[test]
    fn failed_or_cancelled_deployment_never_launches() {
        for message in ["Archive corrupt", "Deployment cancelled"] {
            let error = sequence::<(), ()>(
                &AtomicBool::new(false),
                || anyhow::bail!(message),
                Some(|| panic!("Must not launch after deployment failure")),
            )
            .unwrap_err();
            assert_eq!(error.to_string(), message);
        }
    }

    #[test]
    fn cancellation_during_commit_preserves_installation_without_launching() {
        let cancel = AtomicBool::new(false);
        let (installed, launch) = sequence::<_, ()>(
            &cancel,
            || {
                cancel.store(true, Ordering::Relaxed);
                Ok("committed")
            },
            Some(|| panic!("Must not launch after cancellation")),
        )
        .unwrap();
        assert_eq!(installed, "committed");
        assert!(matches!(launch, Launch::Cancelled));
    }

    #[test]
    fn launch_failure_preserves_successful_installation() {
        let (installed, launch) = sequence::<_, ()>(
            &AtomicBool::new(false),
            || Ok("committed"),
            Some(|| anyhow::bail!("Cannot start executable")),
        )
        .unwrap();
        assert_eq!(installed, "committed");
        assert!(
            matches!(launch, Launch::Failed(error) if error.to_string() == "Cannot start executable")
        );
    }

    #[test]
    fn deployment_only_does_not_start_a_process() {
        let (installed, launch) = sequence(
            &AtomicBool::new(false),
            || Ok("committed"),
            None::<fn() -> Result<()>>,
        )
        .unwrap();
        assert_eq!(installed, "committed");
        assert!(matches!(launch, Launch::NotRequested));
    }
}
