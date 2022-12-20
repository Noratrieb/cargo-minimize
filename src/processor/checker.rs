use std::{collections::BTreeSet, mem};

use self::worklist::Worklist;

use super::AstPath;

#[derive(Debug)]
pub(crate) struct PassController {
    state: PassControllerState,
}

#[derive(Debug)]
enum PassControllerState {
    InitialCollection {
        candidates: Vec<AstPath>,
    },
    Bisecting {
        committed: BTreeSet<AstPath>,
        failed: BTreeSet<AstPath>,
        current: BTreeSet<AstPath>,
        worklist: Worklist,
    },
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

// copied from `core` because who needs stable features anyways
pub const fn div_ceil(lhs: usize, rhs: usize) -> usize {
    let d = lhs / rhs;
    let r = lhs % rhs;
    if r > 0 && rhs > 0 {
        d + 1
    } else {
        d
    }
}

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

impl PassController {
    pub fn new() -> Self {
        Self {
            state: PassControllerState::InitialCollection {
                candidates: Vec::new(),
            },
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
            PassControllerState::Success => unreachable!("Processed after success"),
        }
    }

    pub fn does_not_reproduce(&mut self) {
        match &mut self.state {
            PassControllerState::InitialCollection { candidates } => {
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
                committed: _,
                failed,
                current,
                worklist,
            } => {
                if current.len() == 1 {
                    // We are at a leaf. This is a failure.
                    // FIXME: We should retry the failed ones until a fixpoint is reached.
                    failed.extend(mem::take(current));
                } else {
                    // Split it further and add it to the worklist.
                    let (first_half, second_half) = split_owned(mem::take(current));

                    worklist.push(first_half);
                    worklist.push(second_half);
                }

                self.next_in_worklist()
            }
            PassControllerState::Success => unreachable!("Processed after success"),
        }
    }

    pub fn no_change(&mut self) {
        match &self.state {
            PassControllerState::InitialCollection { candidates } => {
                assert!(
                    candidates.is_empty(),
                    "No change but received candidates: {candidates:?}"
                );
                self.state = PassControllerState::Success;
            }
            PassControllerState::Bisecting { current, .. } => {
                unreachable!("No change while bisecting, current was empty somehow: {current:?}");
            }
            PassControllerState::Success => {}
        }
    }

    pub fn is_finished(&mut self) -> bool {
        match &mut self.state {
            PassControllerState::InitialCollection { .. } => false,
            PassControllerState::Bisecting { .. } => false,
            PassControllerState::Success => true,
        }
    }

    pub fn can_process(&mut self, path: &[String]) -> bool {
        match &mut self.state {
            PassControllerState::InitialCollection { candidates } => {
                candidates.push(AstPath(path.to_owned()));
                true
            }
            PassControllerState::Bisecting { current, .. } => current.contains(path),
            PassControllerState::Success => {
                unreachable!("Processed further after success");
            }
        }
    }

    fn next_in_worklist(&mut self) {
        match &mut self.state {
            PassControllerState::Bisecting {
                current, worklist, ..
            } => match worklist.pop() {
                Some(next) => {
                    *current = next.into_iter().collect();
                }
                None => {
                    self.state = PassControllerState::Success;
                }
            },
            _ => unreachable!("next_in_worklist called on non-bisecting state"),
        }
    }
}
