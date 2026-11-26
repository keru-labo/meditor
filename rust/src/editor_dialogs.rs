use std::{collections::VecDeque, path::PathBuf};

#[derive(Default)]
pub(super) struct EditorDialogs {
    scripted: Option<VecDeque<DialogAnswer>>,
    unexpected: bool,
}

pub(super) enum DialogAnswer {
    File(Option<PathBuf>),
    Files(Vec<PathBuf>),
    Confirm(bool),
}

impl EditorDialogs {
    pub(super) fn supply(&mut self, answers: impl IntoIterator<Item = DialogAnswer>) {
        self.scripted = Some(answers.into_iter().collect());
        self.unexpected = false;
    }

    pub(super) fn consumed(&self) -> bool {
        !self.unexpected && self.scripted.as_ref().is_some_and(VecDeque::is_empty)
    }

    pub(super) fn scripted(&self) -> bool {
        self.scripted.is_some()
    }

    fn take(&mut self, expected: impl FnOnce(&DialogAnswer) -> bool) -> Option<DialogAnswer> {
        let answers = self.scripted.as_mut()?;
        if answers.front().is_some_and(expected) {
            return answers.pop_front();
        }
        self.unexpected = true;
        None
    }

    pub(super) fn file(&mut self, native: impl FnOnce() -> Option<PathBuf>) -> Option<PathBuf> {
        if self.scripted.is_none() {
            return native();
        }
        match self.take(|answer| matches!(answer, DialogAnswer::File(_))) {
            Some(DialogAnswer::File(path)) => path,
            _ => None,
        }
    }

    pub(super) fn files(&mut self, native: impl FnOnce() -> Vec<PathBuf>) -> Vec<PathBuf> {
        if self.scripted.is_none() {
            return native();
        }
        match self.take(|answer| matches!(answer, DialogAnswer::Files(_))) {
            Some(DialogAnswer::Files(paths)) => paths,
            _ => Vec::new(),
        }
    }

    pub(super) fn confirm(&mut self, native: impl FnOnce() -> bool) -> bool {
        if self.scripted.is_none() {
            return native();
        }
        matches!(
            self.take(|answer| matches!(answer, DialogAnswer::Confirm(_))),
            Some(DialogAnswer::Confirm(true))
        )
    }
}
