use std::{borrow::Borrow, collections::BTreeSet, fmt::Debug, mem};

use crate::Options;

use self::worklist::Worklist;

use super::MinimizeEdit;

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
    /// After applying all candidates fails, we know that we have a few bad candidates.
    /// Now our job is to apply all the good candidates as efficiently as possible.
    Bisecting {
        /// These candidates could be applied successfully while still reproducing the issue.
        /// They are now on disk and will be included in all subsequent runs.
        /// This is only used for debugging, we could also just throw them away.
        committed: BTreeSet<MinimizeEdit>,
        /// These candidates failed in isolation and are therefore bad.
        /// This is only used for debugging, we could also just throw them away.
        failed: BTreeSet<MinimizeEdit>,
        /// The set of candidates that we want to apply in this iteration.
        current: Vec<MinimizeEdit>,
        /// The list of `current`s that we want to try in the future.
        worklist: Worklist,
    },
    /// Bisection is over and all candidates were able to be committed or thrown away.
    Success,
}

mod worklist {
    use crate::processor::MinimizeEdit;

    /// A worklist that ensures that the inner list is never empty.
    #[derive(Debug)]
    pub(super) struct Worklist(Vec<Vec<MinimizeEdit>>);

    impl Worklist {
        pub(super) fn new() -> Self {
            Self(Vec::new())
        }

        pub(super) fn push(&mut self, next: Vec<MinimizeEdit>) {
            if !next.is_empty() {
                self.0.push(next);
            }
        }

        pub(super) fn pop(&mut self) -> Option<Vec<MinimizeEdit>> {
            self.0.pop()
        }
    }
}

impl PassController {
    pub fn new(options: Options, edits: Vec<MinimizeEdit>) -> Self {
        Self {
            state: PassControllerState::Bisecting {
                committed: BTreeSet::new(),
                failed: BTreeSet::new(),
                current: edits,
                worklist: Worklist::new(),
            },
            options,
        }
    }

    pub fn reproduces(&mut self) {
        match &mut self.state {
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
            PassControllerState::Bisecting { current, .. } => {
                assert!(current.is_empty(), "there are edits available and yet nothing changed, that's nonsense, there's a bug somewhere (i dont know where)");
                self.state = PassControllerState::Success;
            }
            PassControllerState::Success { .. } => {}
        }
    }

    pub fn is_finished(&mut self) -> bool {
        match &mut self.state {
            PassControllerState::Bisecting { .. } => false,
            PassControllerState::Success { .. } => true,
        }
    }

    pub fn can_process(&mut self, _: &[String]) -> bool {
        false
    }

    /// Checks whether a pass may apply the changes for a minimization site.
    pub fn current_work_items(&mut self) -> &[MinimizeEdit] {
        match &mut self.state {
            PassControllerState::Bisecting { current, .. } => current,
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
