//! Shared file grouping, selection, and commit rules used by every control.
use super::{
    GitUi,
    repository::{Change, Snapshot, conflict},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Side {
    Working,
    Staged,
}
impl Side {
    pub(super) fn is_staged(self) -> bool {
        self == Self::Staged
    }
    pub(super) fn counts_index(self) -> usize {
        if self.is_staged() { 0 } else { 1 }
    }
}
impl From<bool> for Side {
    fn from(staged: bool) -> Self {
        if staged { Self::Staged } else { Self::Working }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct Selection {
    pub path: String,
    pub side: Side,
}
impl Selection {
    pub(super) fn new(path: impl Into<String>, side: Side) -> Self {
        Self {
            path: path.into(),
            side,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Group {
    Conflicts,
    Staged,
    Changes,
}
impl Group {
    pub(super) const ALL: [Self; 3] = [Self::Conflicts, Self::Staged, Self::Changes];
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::Conflicts => "Conflicts",
            Self::Staged => "Staged Changes",
            Self::Changes => "Changes",
        }
    }
    pub(super) fn side(self) -> Side {
        if self == Self::Staged {
            Side::Staged
        } else {
            Side::Working
        }
    }
    pub(super) fn contains(self, change: &Change) -> bool {
        match self {
            Self::Conflicts => conflict(change),
            Self::Staged => change.has_staged(),
            Self::Changes => change.has_working() && !conflict(change),
        }
    }
}
impl Change {
    pub(super) fn has_staged(&self) -> bool {
        self.index != ' ' && self.index != '?' && !conflict(self)
    }
    pub(super) fn has_working(&self) -> bool {
        self.work != ' ' || self.index == '?' || conflict(self)
    }
    pub(super) fn paths(&self) -> Vec<String> {
        let mut paths = vec![self.path.clone()];
        if let Some(old) = &self.old {
            if old != &self.path {
                paths.push(old.clone());
            }
        }
        paths
    }
    pub(super) fn status(&self, side: Side) -> &'static str {
        if conflict(self) {
            "Conflict"
        } else if self.index == '?' {
            "New"
        } else {
            super::presentation::label(if side.is_staged() {
                self.index
            } else {
                self.work
            })
        }
    }
}
impl Snapshot {
    pub(super) fn group(&self, group: Group) -> Vec<&Change> {
        let mut files: Vec<_> = self
            .files
            .iter()
            .filter(|file| group.contains(file))
            .collect();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        files
    }
    pub(super) fn staged_count(&self) -> usize {
        self.files.iter().filter(|file| file.has_staged()).count()
    }
}

impl GitUi {
    pub(super) fn commit_blocker(&self) -> Option<&'static str> {
        if self.busy() {
            Some("Git operation in progress")
        } else if let Some(snapshot) = &self.snapshot {
            if snapshot.files.iter().any(conflict) {
                Some("Resolve conflicts before committing")
            } else if snapshot.staged_count() == 0 {
                Some("Stage files to commit")
            } else if self.commit.trim().is_empty() {
                Some("Enter a commit message")
            } else {
                None
            }
        } else {
            Some("Repository status unavailable")
        }
    }
}
