use std::{borrow::Borrow, collections::BTreeSet, fmt::Debug, mem};

use crate::Options;

use self::worklist::Worklist;

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct AstPath(Vec<String>);

impl Borrow<[String]> for AstPath {
    fn borrow(&self) -> &[String] {
        &self.0
    }
}

impl Debug for AstPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AstPath({:?})", self.0)
    }
}

/// `PassController` is the interface between the passes and the core logic.
/// Its job is to bisect down the minimization sites so that all the ones that can be applied
/// are applied while trying to apply as many as possible in batches.
#[derive(Debug)]
pub(crate) struct PassController {
    state: PassControllerState,
    pub(crate) options: Options,
}

/// The current state of the bisection.
#[derive(Debug)]
enum PassControllerState {
    /// Initially, we have a bunch of candidates (minimization sites) that could be applied.
    /// We collect them in the initial application of the pass where we try to apply all candiates.
    /// If that works, great! We're done. But often it doesn't and we enter the next stage.
    InitialCollection { candidates: Vec<AstPath> },
    /// After applying all candidates fails, we know that we have a few bad candidates.
    /// Now our job is to apply all the good candidates as efficiently as possible.
    Bisecting {
        /// These candidates could be applied successfully while still reproducing the issue.
        /// They are now on disk and will be included in all subsequent runs.
        /// This is only used for debugging, we could also just throw them away.
        committed: BTreeSet<AstPath>,
        /// These candidates failed in isolation and are therefore bad.
        /// This is only used for debugging, we could also just throw them away.
        failed: BTreeSet<AstPath>,
        /// The set of candidates that we want to apply in this iteration.
        current: BTreeSet<AstPath>,
        /// The list of `current`s that we want to try in the future.
        worklist: Worklist,
    },
    /// Bisection is over and all candidates were able to be committed or thrown away.
    Success,
}

mod worklist {
    use super::AstPath;

    /// A worklist that ensures that the inner list is never empty.
    #[derive(Debug)]
    pub(super) struct Worklist(Vec<Vec<AstPath>>);

    impl Worklist {
        pub(super) fn new() -> Self {
            Self(Vec::new())
        }

        pub(super) fn push(&mut self, next: Vec<AstPath>) {
            if !next.is_empty() {
                self.0.push(next);
            }
        }

        pub(super) fn pop(&mut self) -> Option<Vec<AstPath>> {
            self.0.pop()
        }
    }
}

impl PassController {
    pub fn new(options: Options) -> Self {
        Self {
            state: PassControllerState::InitialCollection {
                candidates: Vec::new(),
            },
            options,
        }
    }

    pub fn reproduces(&mut self) {
        match &mut self.state {
            PassControllerState::InitialCollection { .. } => {
                self.state = PassControllerState::Success;
            }
            PassControllerState::Bisecting {
                committed,
                failed: _,
                current,
                worklist: _,
            } => {
                committed.extend(mem::take(current));

                self.next_in_worklist();
            }
            PassControllerState::Success { .. } => unreachable!("Processed after success"),
        }
    }

    /// The changes did not reproduce the regression. Bisect further.
    pub fn does_not_reproduce(&mut self) {
        match &mut self.state {
            PassControllerState::InitialCollection { candidates } => {
                // Applying them all was too much, let's bisect!
                let (current, first_worklist_item) = split_owned(mem::take(candidates));

                let mut worklist = Worklist::new();
                worklist.push(first_worklist_item);

                self.state = PassControllerState::Bisecting {
                    committed: BTreeSet::new(),
                    failed: BTreeSet::new(),
                    current,
                    worklist,
                };
            }
            PassControllerState::Bisecting {
                committed,
                failed,
                current,
                worklist,
            } => {
                debug!(
                    ?committed,
                    ?failed,
                    ?current,
                    ?worklist,
                    "Does not reproduce"
                );

                if current.len() == 1 {
                    // We are at a leaf. This is a failure.
                    failed.extend(mem::take(current));
                } else {
                    // Split it further and add it to the worklist.
                    let (first_half, second_half) = split_owned(mem::take(current));

                    worklist.push(first_half);
                    worklist.push(second_half);
                }

                self.next_in_worklist()
            }
            PassControllerState::Success { .. } => unreachable!("Processed after success"),
        }
    }

    /// The pass did not apply any changes. We're done.
    pub fn no_change(&mut self) {
        match &self.state {
            PassControllerState::InitialCollection { candidates } => {
                assert!(
                    candidates.is_empty(),
                    "No change but received candidates. The responsible pass does not seem to track the ProcessState correctly: {candidates:?}"
                );
                self.state = PassControllerState::Success;
            }
            PassControllerState::Bisecting { current, .. } => {
                unreachable!("Pass said it didn't change anything in the bisection phase, nils forgot what this means: {current:?}");
            }
            PassControllerState::Success { .. } => {}
        }
    }

    pub fn is_finished(&mut self) -> bool {
        match &mut self.state {
            PassControllerState::InitialCollection { .. } => false,
            PassControllerState::Bisecting { .. } => false,
            PassControllerState::Success { .. } => true,
        }
    }

    /// Checks whether a pass may apply the changes for a minimization site.
    pub fn can_process(&mut self, path: &[String]) -> bool {
        match &mut self.state {
            PassControllerState::InitialCollection { candidates } => {
                // For the initial collection, we collect the candidate and apply them all.
                candidates.push(AstPath(path.to_owned()));
                true
            }
            PassControllerState::Bisecting { current, .. } => current.contains(path),
            PassControllerState::Success { .. } => {
                unreachable!("Processed further after success");
            }
        }
    }

    fn next_in_worklist(&mut self) {
        let PassControllerState::Bisecting {
            current, worklist, ..
        } = &mut self.state
        else {
            unreachable!("next_in_worklist called on non-bisecting state");
        };
        match worklist.pop() {
            Some(next) => {
                *current = next.into_iter().collect();
            }
            None => {
                self.state = PassControllerState::Success;
            }
        }
    }
}

// copied from `core` because who needs stable features anyways
// update: still not stabilized because of bikeshedding for div_floor.
pub const fn div_ceil(lhs: usize, rhs: usize) -> usize {
    let d = lhs / rhs;
    let r = lhs % rhs;
    if r > 0 && rhs > 0 {
        d + 1
    } else {
        d
    }
}

/// Splits an owned container in half.
fn split_owned<T, From: IntoIterator<Item = T>, A: FromIterator<T>, B: FromIterator<T>>(
    vec: From,
) -> (A, B) {
    let candidates = vec.into_iter().collect::<Vec<_>>();
    let half = div_ceil(candidates.len(), 2);

    let mut candidates = candidates.into_iter();

    let first_half = candidates.by_ref().take(half).collect();
    let second_half = candidates.collect();

    (first_half, second_half)
}
