//! `corona launcher dmenu`: lines in, the chosen line out

use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

use futures::channel::oneshot;
use gpui_kit::{App, SharedString, assets::IconName};
use rust_i18n::t;

use crate::{
  command::{Accessory, Action, Command, CommandSource, Meta, Outcome, simple},
  matching::Query,
  view::{ListView, View},
};

pub struct Options {
  pub lines: Vec<String>,
  pub prompt: Option<String>,
  pub placeholder: Option<String>,
  /// Ctrl+Enter marks lines, Enter returns the marked ones
  pub multi: bool,
}

/// The choice, or an error when the launcher closes without one
pub type Choice = oneshot::Receiver<Vec<String>>;

struct Lines {
  lines: Rc<Vec<String>>,
  multi: bool,
  marked: Rc<RefCell<BTreeSet<usize>>>,
  answer: Rc<RefCell<Option<oneshot::Sender<Vec<String>>>>>,
}

impl Lines {
  fn finish(answer: &RefCell<Option<oneshot::Sender<Vec<String>>>>, chosen: Vec<String>) {
    if let Some(sender) = answer.borrow_mut().take() {
      let _ = sender.send(chosen);
    }
  }
}

impl CommandSource for Lines {
  fn id(&self) -> SharedString {
    "dmenu".into()
  }

  fn commands(&self, _: &Query, _: &App) -> Vec<Rc<dyn Command>> {
    self
      .lines
      .iter()
      .enumerate()
      .map(|(i, line)| {
        let (marked, answer, lines, multi) = (
          self.marked.clone(),
          self.answer.clone(),
          self.lines.clone(),
          self.multi,
        );
        let title: SharedString = line.clone().into();
        let shown = self.marked.clone();
        simple(
          format!("dmenu:{i}"),
          move |_| Meta {
            title: title.clone(),
            accessories: match shown.borrow().contains(&i) {
              true => vec![Accessory::Icon(IconName::Check)],
              false => Vec::new(),
            },
            ..Default::default()
          },
          move |_| {
            let (choose_marked, choose_answer, choose_lines) =
              (marked.clone(), answer.clone(), lines.clone());
            let mut actions = vec![Action::sync(
              "choose",
              t!("app.launcher.dmenu.choose"),
              move |_, _, _| {
                let marked = choose_marked.borrow();
                let chosen = match marked.is_empty() {
                  true => vec![choose_lines[i].clone()],
                  false => marked.iter().map(|m| choose_lines[*m].clone()).collect(),
                };
                Lines::finish(&choose_answer, chosen);
                Ok(Outcome::close())
              },
            )];
            if multi {
              let marked = marked.clone();
              actions.push(
                Action::sync("mark", t!("app.launcher.dmenu.mark"), move |_, _, _| {
                  let mut marked = marked.borrow_mut();
                  if !marked.remove(&i) {
                    marked.insert(i);
                  }
                  Ok(Outcome::KeepOpen)
                })
                .shortcut("ctrl-enter")
                .keep_open(),
              );
            }
            actions
          },
        )
      })
      .collect()
  }
}

/// The list view and the receiver of the choice; closing without one drops the sender
pub fn view(options: Options) -> (View, Choice) {
  let (sender, receiver) = oneshot::channel();
  let lines = Lines {
    lines: Rc::new(options.lines),
    multi: options.multi,
    marked: Rc::default(),
    answer: Rc::new(RefCell::new(Some(sender))),
  };
  let title = options
    .prompt
    .unwrap_or_else(|| t!("app.launcher.dmenu.title").to_string());
  let placeholder = options
    .placeholder
    .unwrap_or_else(|| t!("app.launcher.dmenu.placeholder").to_string());
  (
    ListView::new(title, placeholder, Rc::new(lines)).into_view(),
    receiver,
  )
}
