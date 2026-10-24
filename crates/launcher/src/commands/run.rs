//! `>` runs a shell command, "Run in terminal" opens a program in the terminal

use std::rc::Rc;

use anyhow::Result;
use corona_config::ConfigProvider;
use corona_desktop::apps;
use gpui_kit::{App, SharedString, assets::IconName};
use rust_i18n::t;

use crate::{
  args::ArgSpec,
  command::{Action, Command, CommandSource, Icon, Meta, Outcome, Scope, simple},
  matching::Query,
  registry,
};

pub fn init(cx: &mut App) {
  registry::register_all([run_command(), run_in_terminal()], cx);
  registry::register_source(Rc::new(TerminalFallback), cx);
}

fn shell(command: &str) -> Vec<String> {
  vec!["sh".into(), "-c".into(), command.into()]
}

fn in_background(command: &str) -> Result<Outcome> {
  apps::spawn("shell", &shell(command), None)?;
  Ok(Outcome::close())
}

fn in_terminal(command: &str, cx: &App) -> Result<Outcome> {
  let mut argv = apps::terminal(&cx.config().launcher.terminal);
  argv.extend(shell(command));
  apps::spawn("terminal", &argv, None)?;
  Ok(Outcome::close())
}

/// Actions for running `command`: in the background first, Ctrl+Enter in a terminal
pub fn actions(command: String) -> Vec<Action> {
  let background = command.clone();
  vec![
    Action::sync("run", t!("app.launcher.run.background"), move |_, _, _| {
      in_background(&background)
    })
    .icon(IconName::Play),
    Action::sync(
      "terminal",
      t!("app.launcher.run.terminal"),
      move |_, _, cx| in_terminal(&command, cx),
    )
    .icon(IconName::SquareTerminal)
    .shortcut("ctrl-enter"),
  ]
}

/// What `>` scopes to: the typed command as one result
struct ShellSource;

impl CommandSource for ShellSource {
  fn id(&self) -> SharedString {
    "run".into()
  }

  fn filters(&self) -> bool {
    true
  }

  fn commands(&self, query: &Query, _: &App) -> Vec<Rc<dyn Command>> {
    let command = query.raw.trim().to_string();
    if command.is_empty() {
      return Vec::new();
    }
    let title = t!("app.launcher.run.title", command = command.clone());
    vec![simple(
      "run:query",
      move |_| Meta {
        title: title.clone().into(),
        icon: Icon::Tile(IconName::SquareTerminal, 140.),
        ..Default::default()
      },
      move |_| actions(command.clone()),
    )]
  }
}

struct RunCommand;

impl Command for RunCommand {
  fn id(&self) -> SharedString {
    "run:command".into()
  }

  fn meta(&self, _: &App) -> Meta {
    Meta {
      title: t!("app.launcher.run.command").into(),
      icon: Icon::Tile(IconName::SquareTerminal, 140.),
      keywords: vec!["shell".into(), "exec".into()],
      args: vec![ArgSpec::text(t!("app.launcher.run.command_arg"))],
      ..Default::default()
    }
  }

  fn describe(&self, args: &crate::Args, _: &App) -> Option<SharedString> {
    let command = args.text(0)?;
    Some(t!("app.launcher.run.title", command = command).into())
  }

  fn actions(&self, _: &App) -> Vec<Action> {
    vec![
      Action::sync("run", t!("app.launcher.run.background"), |args, _, _| {
        in_background(args.text(0).unwrap_or_default())
      }),
      Action::sync(
        "terminal",
        t!("app.launcher.run.terminal"),
        |args, _, cx| in_terminal(args.text(0).unwrap_or_default(), cx),
      )
      .shortcut("ctrl-enter"),
    ]
  }

  fn scope(&self, _: &App) -> Option<Scope> {
    Some(Scope {
      prefix: ">".into(),
      label: t!("app.launcher.run.chip").into(),
      placeholder: t!("app.launcher.run.placeholder").into(),
      source: Rc::new(ShellSource),
    })
  }
}

fn run_command() -> Rc<dyn Command> {
  Rc::new(RunCommand)
}

fn run_in_terminal() -> Rc<dyn Command> {
  simple(
    "run:terminal",
    |_| Meta {
      title: t!("app.launcher.run.terminal_program").into(),
      icon: Icon::Tile(IconName::SquareTerminal, 160.),
      keywords: vec!["tui".into(), "console".into()],
      args: vec![ArgSpec::text(t!("app.launcher.run.program"))],
      ..Default::default()
    },
    |_| {
      vec![Action::sync(
        "open",
        t!("app.launcher.run.terminal"),
        |args, _, cx| {
          let program = args.text(0).unwrap_or_default();
          let mut argv = apps::terminal(&cx.config().launcher.terminal);
          argv.extend(program.split_whitespace().map(String::from));
          apps::spawn("terminal", &argv, None)?;
          Ok(Outcome::close())
        },
      )]
    },
  )
}

/// "Run in terminal" for a query nothing matched
struct TerminalFallback;

impl CommandSource for TerminalFallback {
  fn id(&self) -> SharedString {
    "fallback:terminal".into()
  }

  fn kind(&self) -> crate::SourceKind {
    crate::SourceKind::Fallback
  }

  fn commands(&self, query: &Query, _: &App) -> Vec<Rc<dyn Command>> {
    let command = query.raw.trim().to_string();
    let title = t!("app.launcher.run.in_terminal", command = command.clone());
    vec![simple(
      "fallback:terminal",
      move |_| Meta {
        title: title.clone().into(),
        icon: Icon::Tile(IconName::SquareTerminal, 140.),
        ..Default::default()
      },
      move |_| {
        let command = command.clone();
        vec![Action::sync(
          "terminal",
          t!("app.launcher.run.terminal"),
          move |_, _, cx| in_terminal(&command, cx),
        )]
      },
    )]
  }
}
