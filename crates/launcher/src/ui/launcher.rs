use std::{collections::HashSet, rc::Rc, time::Duration};

use anyhow::Result;
use corona_config::ConfigProvider;
use gpui_kit::{
  AnyView, App, AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement,
  KeyDownEvent, ListAlignment, ListState, MouseButton, MouseMoveEvent, ParentElement, Render,
  SharedString, Styled, Subscription, Task, Window,
  base::input::{InputEvent, InputState},
  div, px,
};
use rust_i18n::t;

use super::{
  Back, Complete, CompleteBack, CycleFilter, LauncherState, Open, PageDown, PageUp, Pop, Run,
  RunKeepOpen, RunSecondary, SelectLeft, SelectNext, SelectPrev, SelectRight, Submit,
  ToggleActions, TogglePreview, form::FormPage, hud, style, window_size,
};
use crate::{
  args::{ArgSpec, Args},
  command::{Action, Command, Confirm, Hud, Outcome, Scope, Section, Toast, ToastKind},
  matching::{Query, match_text},
  paste,
  registry::{self, Registry, Results, Row},
  state,
  view::{CustomView, DetailView, Layout, ListView, View},
};

const TOAST_TIME: Duration = Duration::from_secs(4);
const PAGE: usize = 8;

pub(super) enum Item {
  Header(Section),
  Row(usize),
}

pub(super) enum Body {
  Root,
  List(ListView),
  Form(Entity<FormPage>),
  Detail(DetailView),
  Custom { view: CustomView, element: AnyView },
}

pub(super) struct Page {
  pub body: Body,
  pub title: Option<SharedString>,
  pub query: String,
  pub scope: Option<Scope>,
  pub rows: Vec<Row>,
  pub items: Vec<Item>,
  pub selected: usize,
  pub selected_id: Option<SharedString>,
  pub args: Option<ArgsRow>,
  pub list: ListState,
  /// Which completion Tab showed last
  pub completion: usize,
  _subscriptions: Vec<Subscription>,
}

impl Page {
  fn new(body: Body, title: Option<SharedString>, query: String) -> Self {
    Self {
      body,
      title,
      query,
      scope: None,
      rows: Vec::new(),
      items: Vec::new(),
      selected: 0,
      selected_id: None,
      args: None,
      list: ListState::new(0, ListAlignment::Top, px(200.)),
      completion: 0,
      _subscriptions: Vec::new(),
    }
  }

  pub fn layout(&self) -> Layout {
    match &self.body {
      Body::List(v) => v.layout,
      _ => Layout::List,
    }
  }

  pub fn columns(&self) -> usize {
    match self.layout() {
      Layout::Grid { columns, .. } => columns.max(1),
      Layout::List => 1,
    }
  }

  pub fn selected_row(&self) -> Option<&Row> {
    self.rows.get(self.selected)
  }

  /// Shows the search field
  pub fn searches(&self) -> bool {
    match &self.body {
      Body::Root | Body::List(_) => true,
      Body::Custom { view, .. } => view.search,
      Body::Form(_) | Body::Detail(_) => false,
    }
  }

  pub fn placeholder(&self) -> SharedString {
    if let Some(scope) = &self.scope {
      return scope.placeholder.clone();
    }
    match &self.body {
      Body::List(v) => v.placeholder.clone(),
      _ => t!("app.launcher.placeholder").into(),
    }
  }

  pub fn previews(&self) -> bool {
    match &self.body {
      Body::Root => true,
      Body::List(v) => v.preview,
      _ => false,
    }
  }
}

/// Fields after the search bar for a command's arguments
pub(super) struct ArgsRow {
  pub command: Rc<dyn Command>,
  pub specs: Vec<ArgSpec>,
  pub inputs: Vec<Entity<InputState>>,
  pub focused: usize,
  pub error: Option<(usize, SharedString)>,
}

pub(super) struct Panel {
  pub command: Option<Rc<dyn Command>>,
  pub args: Args,
  pub actions: Vec<Action>,
  pub input: Entity<InputState>,
  pub selected: usize,
  _subscription: Subscription,
}

impl Panel {
  pub fn visible(&self, cx: &App) -> Vec<(usize, &Action)> {
    let query = self.input.read(cx).value().to_lowercase();
    self
      .actions
      .iter()
      .enumerate()
      .filter(|(_, a)| query.is_empty() || match_text(&a.title, &query).is_some())
      .collect()
  }
}

/// A run waiting on something: its command, action and arguments
type Deferred = (Option<Rc<dyn Command>>, Action, Args);

pub(super) enum Pending {
  Run {
    command: Option<Rc<dyn Command>>,
    action: Box<Action>,
    args: Args,
    keep_open: bool,
  },
  /// Leave a form with edits
  Discard,
}

pub(super) struct ConfirmState {
  pub pending: Pending,
  pub confirm: Confirm,
  /// Enter confirms only after Tab moved focus onto the button
  pub on_confirm: bool,
  pub dont_ask: bool,
}

pub struct Launcher {
  focus: FocusHandle,
  pub(super) input: Entity<InputState>,
  pub(super) pages: Vec<Page>,
  /// Opened into a command or view: leaving its first page closes
  pub(super) external: bool,
  pub(super) panel: Option<Panel>,
  pub(super) confirm: Option<ConfirmState>,
  pub(super) toast: Option<Toast>,
  toast_timer: Option<Task<()>>,
  pub(super) preview: bool,
  pub(super) busy: usize,
  /// What the failure toast's Retry runs
  retry: Option<Deferred>,
  was_active: bool,
  /// Where the pointer first showed up outside the card; moving on from there closes
  pointer: Option<gpui_kit::Point<gpui_kit::Pixels>>,
  /// The pointer was over the surface, leaving it for another monitor closes
  was_hovered: bool,
  skip_confirm: HashSet<String>,
  _subscriptions: Vec<Subscription>,
}

impl Launcher {
  pub fn new(open: Open, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let input = cx.new(|cx| InputState::new(window, cx));
    let subscriptions = vec![
      cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
        if let InputEvent::Change = event {
          this.on_query(window, cx);
        }
      }),
      cx.observe_window_activation(window, |this: &mut Self, window, cx| {
        if window.is_window_active() {
          this.was_active = true;
        } else if this.was_active && cx.config().launcher.close_on_unfocus {
          close_in(window, cx);
        }
      }),
    ];
    let mut this = Self {
      focus: cx.focus_handle(),
      input,
      pages: Vec::new(),
      external: false,
      panel: None,
      confirm: None,
      toast: None,
      toast_timer: None,
      preview: cx.config().launcher.preview,
      busy: 0,
      retry: None,
      was_active: false,
      pointer: None,
      was_hovered: false,
      skip_confirm: state::load("confirm_skips"),
      _subscriptions: subscriptions,
    };
    this.reset(open, window, cx);
    this
  }

  pub fn focus(&self, window: &mut Window, cx: &mut App) {
    if let Some(Body::Form(form)) = self.pages.last().map(|p| &p.body) {
      let form = form.clone();
      form.update(cx, |form, cx| form.focus_first(window, cx));
      return;
    }
    if let Some(args) = self.pages.last().and_then(|p| p.args.as_ref())
      && let Some(input) = args.inputs.get(args.focused)
    {
      input.update(cx, |input, cx| input.focus(window, cx));
      return;
    }
    self.input.update(cx, |input, cx| input.focus(window, cx));
  }

  /// Starts over as `open` says
  pub fn reset(&mut self, open: Open, window: &mut Window, cx: &mut Context<Self>) {
    for page in self.pages.drain(..) {
      close_source(&page, cx);
    }
    self.panel = None;
    self.confirm = None;
    self.external = false;
    match open {
      Open::Root { query } => {
        self.push_page(Page::new(Body::Root, None, query), window, cx);
      }
      Open::View(view) => {
        self.external = true;
        self.push(view, window, cx);
      }
      Open::Command { id, query } => {
        self.push_page(Page::new(Body::Root, None, String::new()), window, cx);
        let command = cx.global::<Registry>().find(&id, cx);
        match command {
          Some(command) => {
            self.external = true;
            let specs = command.meta(cx).args;
            let args = Args::parse(&specs, &query).unwrap_or_default();
            if let Some(action) = command.actions(cx).into_iter().next() {
              self.run(Some(command), action, args, false, window, cx);
            }
          }
          None => self.show_toast(
            Toast {
              kind: ToastKind::Failure,
              title: t!("app.launcher.unknown_command", id = id).into(),
              message: None,
              action: None,
            },
            cx,
          ),
        }
      }
    }
  }

  fn page(&self) -> &Page {
    self.pages.last().expect("the launcher always has a page")
  }

  fn page_mut(&mut self) -> &mut Page {
    self
      .pages
      .last_mut()
      .expect("the launcher always has a page")
  }

  fn push_page(&mut self, mut page: Page, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(current) = self.pages.last_mut() {
      current.query = self.input.read(cx).value().to_string();
    }
    let weak = cx.entity().downgrade();
    let notify: Rc<dyn Fn(&mut App)> = Rc::new(move |cx| {
      let _ = weak.update(cx, |this, cx| this.refresh(cx));
    });
    page._subscriptions = match &page.body {
      Body::Root => {
        let registry = cx.global::<Registry>().sources().to_vec();
        registry
          .iter()
          .flat_map(|s| s.observe(notify.clone(), cx))
          .collect()
      }
      Body::List(view) => {
        let source = view.source.clone();
        source.open(cx);
        source.observe(notify, cx)
      }
      _ => Vec::new(),
    };
    let query = page.query.clone();
    let placeholder = page.placeholder();
    self.pages.push(page);
    self.input.update(cx, |input, cx| {
      input.set_value(query, window, cx);
      input.set_placeholder(placeholder, window, cx);
    });
    self.refresh(cx);
    self.focus(window, cx);
  }

  pub(super) fn push(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
    let title = Some(view.title());
    let page = match view {
      View::List(list) => {
        let query = list.query.clone();
        Page::new(Body::List(list), title, query)
      }
      View::Form(form) => {
        let form = cx.new(|cx| FormPage::new(form, window, cx));
        Page::new(Body::Form(form), title, String::new())
      }
      View::Detail(detail) => Page::new(Body::Detail(detail), title, String::new()),
      View::Custom(view) => {
        let element = (view.build)(window, cx);
        Page::new(Body::Custom { view, element }, title, String::new())
      }
    };
    self.push_page(page, window, cx);
  }

  /// Back to the previous page as it was, or close when there is none
  pub(super) fn pop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    if self.pages.len() <= 1 || (self.external && self.pages.len() <= 2 && self.root_hidden()) {
      return close_in(window, cx);
    }
    let page = self.pages.pop().expect("more than one page");
    close_source(&page, cx);
    if let Body::Form(form) = &page.body
      && let Some(cancel) = form.read(cx).cancel.clone()
    {
      cancel(cx);
    }
    let (query, placeholder) = (self.page().query.clone(), self.page().placeholder());
    self.input.update(cx, |input, cx| {
      input.set_value(query, window, cx);
      input.set_placeholder(placeholder, window, cx);
    });
    self.refresh(cx);
    self.focus(window, cx);
  }

  /// Opened into a command: the root page under it never shows
  fn root_hidden(&self) -> bool {
    self.external && matches!(self.pages.first().map(|p| &p.body), Some(Body::Root))
  }

  fn on_query(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let text = self.input.read(cx).value().to_string();
    let page = self.page_mut();
    page.query = text.clone();
    page.args = None;
    page.completion = 0;
    if matches!(page.body, Body::Root) && page.scope.is_none() {
      let scopes = cx.global::<Registry>().scopes(cx);
      if let Some((scope, rest)) = find_scope(&scopes, &text) {
        let placeholder = scope.placeholder.clone();
        self.page_mut().scope = Some(scope);
        self.page_mut().query = rest.clone();
        self.input.update(cx, |input, cx| {
          input.set_value(rest, window, cx);
          input.set_placeholder(placeholder, window, cx);
        });
      }
    }
    if let Body::Custom { view, .. } = &self.page().body
      && let Some(on_query) = view.on_query.clone()
    {
      on_query(&text, cx);
    }
    self.refresh(cx);
  }

  /// Recomputes the rows of the current page, keeping the selection on the same id
  pub(super) fn refresh(&mut self, cx: &mut Context<Self>) {
    let page = self.pages.last().expect("the launcher always has a page");
    let query = Query::new(page.query.clone());
    let results = match (&page.body, &page.scope) {
      (Body::Root, Some(scope)) => Results {
        evaluated: Vec::new(),
        sections: vec![(
          Section::Results,
          registry::rank_commands(
            scope.source.commands(&query, cx),
            &query,
            scope.source.filters(),
            cx,
          ),
        )],
      },
      (Body::Root, None) => registry::rank(&query, cx),
      (Body::List(view), _) => Results {
        evaluated: Vec::new(),
        sections: vec![(
          Section::Results,
          registry::rank_commands(
            view.source.commands(&query, cx),
            &query,
            view.source.filters(),
            cx,
          ),
        )],
      },
      _ => Results::default(),
    };
    let headers = matches!(page.body, Body::Root) && page.scope.is_none();

    let mut rows = Vec::new();
    let mut items = Vec::new();
    for row in results.evaluated {
      items.push(Item::Row(rows.len()));
      rows.push(row);
    }
    for (section, section_rows) in results.sections {
      if headers && !section_rows.is_empty() {
        items.push(Item::Header(section));
      }
      for row in section_rows {
        items.push(Item::Row(rows.len()));
        rows.push(row);
      }
    }

    let page = self.page_mut();
    let selected = page
      .selected_id
      .as_ref()
      .and_then(|id| rows.iter().position(|r| &r.id() == id))
      .unwrap_or(0)
      .min(rows.len().saturating_sub(1));
    let count = match page.layout() {
      Layout::Grid { columns, .. } => rows.len().div_ceil(columns.max(1)),
      Layout::List => items.len(),
    };
    page.rows = rows;
    page.items = items;
    page.list.reset(count);
    self.select(selected, cx);
  }

  pub(super) fn select(&mut self, index: usize, cx: &mut Context<Self>) {
    let page = self.page_mut();
    page.selected = index.min(page.rows.len().saturating_sub(1));
    let before = page.selected_id.take();
    page.selected_id = page.selected_row().map(Row::id);
    if page.selected_id != before
      && let Some(command) = page.selected_row().map(|r| r.command.clone())
    {
      command.on_select(cx);
    }
    let page = self.page_mut();
    let item = match page.layout() {
      Layout::Grid { columns, .. } => page.selected / columns.max(1),
      Layout::List => page
        .items
        .iter()
        .position(|i| matches!(i, Item::Row(r) if *r == page.selected))
        .unwrap_or(0),
    };
    // reveal the section header above the first row too
    let reveal = if page.selected == 0 { 0 } else { item };
    page.list.scroll_to_reveal_item(reveal);
    cx.notify();
  }

  fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
    let n = self.page().rows.len();
    if n == 0 {
      return;
    }
    let next = (self.page().selected as isize + delta).clamp(0, n as isize - 1);
    self.select(next as usize, cx);
  }

  /// Arguments typed inline for the selected row, or the error to show instead of running
  pub(super) fn inline_args(&self, row: &Row) -> Result<Args> {
    if row.meta.args.is_empty() || row.meta.args_row {
      return Ok(Args::default());
    }
    Args::parse(&row.meta.args, row.found.args.as_deref().unwrap_or(""))
  }

  fn run_selected(
    &mut self,
    secondary: bool,
    keep_open: bool,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    if let Some(Body::Detail(detail)) = self.pages.last().map(|p| &p.body) {
      let index = secondary as usize;
      if let Some(action) = detail.actions.get(index).cloned() {
        self.run(None, action, Args::default(), keep_open, window, cx);
      }
      return;
    }
    if let Some(args) = &self.page().args {
      let fields: Vec<String> = args
        .inputs
        .iter()
        .map(|i| i.read(cx).value().to_string())
        .collect();
      let command = args.command.clone();
      match Args::from_fields(&args.specs, &fields) {
        Ok(parsed) => {
          let action = command.actions(cx).into_iter().nth(secondary as usize);
          if let Some(action) = action {
            self.run(Some(command), action, parsed, keep_open, window, cx);
          }
        }
        Err(e) => {
          let index = args
            .specs
            .iter()
            .zip(&fields)
            .position(|(s, f)| !s.optional && f.trim().is_empty())
            .unwrap_or(0);
          if let Some(args) = self.page_mut().args.as_mut() {
            args.error = Some((index, e.to_string().into()));
            args.focused = index;
          }
          self.focus(window, cx);
          cx.notify();
        }
      }
      return;
    }
    let Some(row) = self.page().selected_row().cloned() else {
      return;
    };
    if row.meta.args_row && row.meta.args.iter().any(|a| !a.optional) {
      return self.open_args(&row, window, cx);
    }
    let Ok(args) = self.inline_args(&row) else {
      // invalid inline arguments leave nothing to run
      return;
    };
    let action = row.command.actions(cx).into_iter().nth(secondary as usize);
    if let Some(action) = action {
      self.run(Some(row.command), action, args, keep_open, window, cx);
    }
  }

  fn open_args(&mut self, row: &Row, window: &mut Window, cx: &mut Context<Self>) {
    let specs: Vec<ArgSpec> = row
      .meta
      .args
      .iter()
      .take(style::MAX_ARGS)
      .cloned()
      .collect();
    let inputs = specs
      .iter()
      .map(|spec| {
        let name = spec.name.clone();
        cx.new(|cx| InputState::new(window, cx).placeholder(name))
      })
      .collect();
    self.page_mut().args = Some(ArgsRow {
      command: row.command.clone(),
      specs,
      inputs,
      focused: 0,
      error: None,
    });
    self.focus(window, cx);
    cx.notify();
  }

  /// Runs `action`, asking first when it wants a confirmation
  pub(super) fn run(
    &mut self,
    command: Option<Rc<dyn Command>>,
    action: Action,
    args: Args,
    keep_open: bool,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    if let Some(confirm) = action.confirm.clone()
      && !self.skip_confirm.contains(action.id.as_ref())
    {
      self.panel = None;
      self.confirm = Some(ConfirmState {
        pending: Pending::Run {
          command,
          action: Box::new(action),
          args,
          keep_open,
        },
        confirm,
        on_confirm: false,
        dont_ask: false,
      });
      cx.notify();
      return;
    }
    self.execute(command, action, args, keep_open, window, cx);
  }

  fn execute(
    &mut self,
    command: Option<Rc<dyn Command>>,
    action: Action,
    args: Args,
    keep_open: bool,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    self.panel = None;
    if let Some(command) = &command {
      registry::record_use(&command.id(), cx);
    }
    let task = (action.run)(&args, window, cx);
    let keep_open = keep_open || action.keep_open;
    self.busy += 1;
    cx.notify();
    cx.spawn_in(window, async move |this, cx| {
      let result = task.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.busy -= 1;
        this.finish(result, command, action, args, keep_open, window, cx);
      });
    })
    .detach();
  }

  #[allow(clippy::too_many_arguments)]
  fn finish(
    &mut self,
    result: Result<Outcome>,
    command: Option<Rc<dyn Command>>,
    action: Action,
    args: Args,
    keep_open: bool,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let outcome = match result {
      Ok(outcome) => outcome,
      Err(e) => {
        tracing::warn!("launcher action {} failed: {e:#}", action.id);
        // the toast's button runs `retry`
        let retry = Action::new("retry", t!("app.launcher.retry"), |_, _, _| {
          Task::ready(Ok(Outcome::KeepOpen))
        });
        self.retry = Some((command, action, args));
        return self.show_toast(
          Toast {
            kind: ToastKind::Failure,
            title: t!("app.launcher.failed").into(),
            message: Some(format!("{e:#}").into()),
            action: Some(retry),
          },
          cx,
        );
      }
    };
    match outcome {
      Outcome::Close(hud) if keep_open => {
        if let Some(hud) = hud {
          self.show_toast(
            Toast {
              kind: ToastKind::Success,
              title: hud.text,
              message: None,
              action: None,
            },
            cx,
          );
        }
        self.refresh(cx);
      }
      Outcome::Close(hud) => {
        close_in(window, cx);
        if let Some(hud) = hud {
          hud::show(hud, cx);
        }
      }
      Outcome::KeepOpen => self.refresh(cx),
      Outcome::Push(view) => self.push(view, window, cx),
      Outcome::Pop => self.pop(window, cx),
      Outcome::Paste(clip) => {
        close_in(window, cx);
        paste::paste(clip, cx);
      }
      Outcome::Copy(clip) => {
        paste::copy(&clip, cx);
        let hud = Hud::new(t!("app.launcher.hud.copied"));
        if keep_open {
          self.show_toast(
            Toast {
              kind: ToastKind::Success,
              title: hud.text,
              message: None,
              action: None,
            },
            cx,
          );
        } else {
          close_in(window, cx);
          hud::show(hud, cx);
        }
      }
      Outcome::Toast(toast) => self.show_toast(toast, cx),
    }
  }

  pub(super) fn show_toast(&mut self, toast: Toast, cx: &mut Context<Self>) {
    let sticky = toast.kind == ToastKind::Progress || toast.action.is_some();
    self.toast = Some(toast);
    self.toast_timer = (!sticky).then(|| {
      cx.spawn(async move |this, cx| {
        cx.background_executor().timer(TOAST_TIME).await;
        let _ = this.update(cx, |this, cx| {
          this.toast = None;
          cx.notify();
        });
      })
    });
    cx.notify();
  }

  fn open_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let (command, args, actions) = match &self.page().body {
      Body::Detail(detail) => (None, Args::default(), detail.actions.clone()),
      _ => {
        let Some(row) = self.page().selected_row().cloned() else {
          return;
        };
        let args = self.inline_args(&row).unwrap_or_default();
        let mut actions = row.command.actions(cx);
        actions.extend(crate::common::actions(row.id(), cx));
        (Some(row.command), args, actions)
      }
    };
    if actions.is_empty() {
      return;
    }
    let input =
      cx.new(|cx| InputState::new(window, cx).placeholder(t!("app.launcher.search_actions")));
    let subscription = cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
      if let (InputEvent::Change, Some(panel)) = (event, this.panel.as_mut()) {
        panel.selected = 0;
        cx.notify();
      }
    });
    input.update(cx, |input, cx| input.focus(window, cx));
    self.panel = Some(Panel {
      command,
      args,
      actions,
      input,
      selected: 0,
      _subscription: subscription,
    });
    cx.notify();
  }

  fn run_panel(&mut self, keep_open: bool, window: &mut Window, cx: &mut Context<Self>) {
    let Some(panel) = &self.panel else {
      return;
    };
    let Some((_, action)) = panel
      .visible(cx)
      .get(panel.selected)
      .map(|(i, a)| (*i, (*a).clone()))
    else {
      return;
    };
    let (command, args) = (panel.command.clone(), panel.args.clone());
    self.panel = None;
    self.focus(window, cx);
    self.run(command, action, args, keep_open, window, cx);
  }

  fn answer_confirm(&mut self, yes: bool, window: &mut Window, cx: &mut Context<Self>) {
    let Some(confirm) = self.confirm.take() else {
      return;
    };
    cx.notify();
    if !yes {
      return;
    }
    match confirm.pending {
      Pending::Run {
        command,
        action,
        args,
        keep_open,
      } => {
        if confirm.dont_ask {
          self.skip_confirm.insert(action.id.to_string());
          let _ = state::save("confirm_skips", &self.skip_confirm);
        }
        self.execute(command, *action, args, keep_open, window, cx);
      }
      Pending::Discard => self.pop(window, cx),
    }
  }

  fn complete(&mut self, back: bool, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(args) = self.page_mut().args.as_mut() {
      let n = args.inputs.len();
      args.focused = match back {
        false => (args.focused + 1) % n,
        true => (args.focused + n - 1) % n,
      };
      self.focus(window, cx);
      cx.notify();
      return;
    }
    let Some(row) = self.page().selected_row().cloned() else {
      return;
    };
    if let Some(text) = row.command.complete(cx) {
      self
        .input
        .update(cx, |input, cx| input.set_value(text.clone(), window, cx));
      self.page_mut().query = text;
      return self.refresh(cx);
    }
    if row.meta.args.is_empty() {
      return;
    }
    if row.meta.args_row {
      return self.open_args(&row, window, cx);
    }
    let name = row.meta.title.to_lowercase();
    let text = match &row.found.args {
      None => format!("{name} "),
      Some(typed) => {
        // complete the argument being typed, Tab again cycles
        let spec = row.meta.args.last().expect("not empty");
        let done = typed.rsplit_once(char::is_whitespace);
        let (before, current) = done.map_or(("", typed.as_str()), |(b, c)| (b, c));
        let page = self.page_mut();
        let prefix = if page.completion == 0 {
          current.to_string()
        } else {
          String::new()
        };
        let options = spec.completions(&prefix);
        if options.is_empty() {
          return;
        }
        let pick = &options[page.completion % options.len()];
        page.completion += 1;
        let head = self
          .page()
          .query
          .strip_suffix(typed.as_str())
          .unwrap_or(&self.page().query)
          .to_string();
        match before.is_empty() {
          true => format!("{head}{pick}"),
          false => format!("{head}{before} {pick}"),
        }
      }
    };
    let completion = self.page().completion;
    self
      .input
      .update(cx, |input, cx| input.set_value(text.clone(), window, cx));
    self.page_mut().query = text;
    self.refresh(cx);
    self.page_mut().completion = completion;
  }

  fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    if self.confirm.is_some() {
      return self.answer_confirm(false, window, cx);
    }
    if self.panel.take().is_some() {
      self.focus(window, cx);
      return cx.notify();
    }
    if self.page_mut().args.take().is_some() {
      self.focus(window, cx);
      return cx.notify();
    }
    if let Body::Form(form) = &self.page().body
      && form.read(cx).dirty(cx)
    {
      self.confirm = Some(ConfirmState {
        pending: Pending::Discard,
        confirm: Confirm {
          title: t!("app.launcher.discard.title").into(),
          message: t!("app.launcher.discard.message").into(),
          button: t!("app.launcher.discard.button").into(),
          skippable: false,
        },
        on_confirm: false,
        dont_ask: false,
      });
      return cx.notify();
    }
    self.pop(window, cx);
  }

  /// Backspace on an empty query drops the prefix chip instead of a character; action
  /// shortcuts of the selected row run without opening the panel
  fn on_key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
    if e.keystroke.key == "backspace" && self.input.read(cx).value().is_empty() {
      if self.page_mut().scope.take().is_some() {
        cx.stop_propagation();
        let placeholder = self.page().placeholder();
        self.input.update(cx, |input, cx| {
          input.set_placeholder(placeholder, window, cx)
        });
        self.page_mut().query = String::new();
        self.refresh(cx);
      }
      return;
    }
    let modified = e.keystroke.modifiers.control || e.keystroke.modifiers.alt;
    if !modified || self.panel.is_some() || self.confirm.is_some() {
      return;
    }
    let keys = e.keystroke.unparse();
    let Some(row) = self.page().selected_row().cloned() else {
      return;
    };
    let mut actions = row.command.actions(cx);
    actions.extend(crate::common::actions(row.id(), cx));
    if let Some(action) = actions
      .into_iter()
      .skip(1)
      .find(|a| a.shortcut.as_deref() == Some(keys.as_str()))
    {
      cx.stop_propagation();
      let args = self.inline_args(&row).unwrap_or_default();
      self.run(Some(row.command), action, args, false, window, cx);
    }
  }

  fn on_select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
    if let Some(panel) = self.panel.as_mut() {
      let n = panel.visible(cx).len();
      panel.selected = (panel.selected + 1).min(n.saturating_sub(1));
      return cx.notify();
    }
    if self.form_focused() {
      return cx.propagate();
    }
    let columns = self.page().columns() as isize;
    self.step(columns, cx);
  }

  fn on_select_prev(&mut self, _: &SelectPrev, _: &mut Window, cx: &mut Context<Self>) {
    if let Some(panel) = self.panel.as_mut() {
      panel.selected = panel.selected.saturating_sub(1);
      return cx.notify();
    }
    if self.form_focused() {
      return cx.propagate();
    }
    let columns = self.page().columns() as isize;
    self.step(-columns, cx);
  }

  fn on_select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
    match self.page().layout() {
      Layout::Grid { .. } if self.panel.is_none() => self.step(-1, cx),
      _ => cx.propagate(),
    }
  }

  fn on_select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
    match self.page().layout() {
      Layout::Grid { .. } if self.panel.is_none() => self.step(1, cx),
      _ => cx.propagate(),
    }
  }

  fn on_page_down(&mut self, _: &PageDown, _: &mut Window, cx: &mut Context<Self>) {
    let rows = (PAGE * self.page().columns()) as isize;
    self.step(rows, cx);
  }

  fn on_page_up(&mut self, _: &PageUp, _: &mut Window, cx: &mut Context<Self>) {
    let rows = (PAGE * self.page().columns()) as isize;
    self.step(-rows, cx);
  }

  /// Forms move between fields with their own keys
  fn form_focused(&self) -> bool {
    matches!(self.page().body, Body::Form(_)) && self.panel.is_none()
  }

  fn on_run(&mut self, _: &Run, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(confirm) = &self.confirm {
      let yes = confirm.on_confirm;
      return self.answer_confirm(yes, window, cx);
    }
    if self.panel.is_some() {
      return self.run_panel(false, window, cx);
    }
    if let Body::Form(form) = &self.page().body {
      let form = form.clone();
      if form.update(cx, |form, cx| form.on_enter(window, cx)) {
        return self.submit(window, cx);
      }
      return;
    }
    self.run_selected(false, false, window, cx);
  }

  fn on_run_secondary(&mut self, _: &RunSecondary, window: &mut Window, cx: &mut Context<Self>) {
    if matches!(self.page().body, Body::Form(_)) {
      return self.submit(window, cx);
    }
    self.run_selected(true, false, window, cx);
  }

  fn on_run_keep_open(&mut self, _: &RunKeepOpen, window: &mut Window, cx: &mut Context<Self>) {
    if self.panel.is_some() {
      return self.run_panel(true, window, cx);
    }
    self.run_selected(false, true, window, cx);
  }

  fn on_submit(&mut self, _: &Submit, window: &mut Window, cx: &mut Context<Self>) {
    if matches!(self.page().body, Body::Form(_)) {
      self.submit(window, cx);
    }
  }

  fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Body::Form(form) = &self.page().body else {
      return;
    };
    let form = form.clone();
    let Some((values, submit)) = form.update(cx, |form, cx| form.validate(window, cx)) else {
      return;
    };
    let action = Action::new("submit", t!("app.launcher.submit"), move |_, window, cx| {
      submit(values.clone(), window, cx)
    });
    self.execute(None, action, Args::default(), false, window, cx);
  }

  fn on_complete(&mut self, _: &Complete, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(confirm) = self.confirm.as_mut() {
      confirm.on_confirm = !confirm.on_confirm;
      return cx.notify();
    }
    if matches!(self.page().body, Body::Form(_)) {
      return cx.propagate();
    }
    self.complete(false, window, cx);
  }

  fn on_complete_back(&mut self, _: &CompleteBack, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(confirm) = self.confirm.as_mut() {
      confirm.on_confirm = !confirm.on_confirm;
      return cx.notify();
    }
    if matches!(self.page().body, Body::Form(_)) {
      return cx.propagate();
    }
    self.complete(true, window, cx);
  }

  fn on_back(&mut self, _: &Back, window: &mut Window, cx: &mut Context<Self>) {
    self.back(window, cx);
  }

  fn on_pop(&mut self, _: &Pop, window: &mut Window, cx: &mut Context<Self>) {
    self.pop(window, cx);
  }

  fn on_toggle_actions(&mut self, _: &ToggleActions, window: &mut Window, cx: &mut Context<Self>) {
    if self.panel.take().is_some() {
      self.focus(window, cx);
      return cx.notify();
    }
    self.open_panel(window, cx);
  }

  fn on_cycle_filter(&mut self, _: &CycleFilter, _: &mut Window, cx: &mut Context<Self>) {
    self.cycle_filter(cx);
  }

  /// The next option of the view's dropdown
  pub(super) fn cycle_filter(&mut self, cx: &mut Context<Self>) {
    let Body::List(view) = &self.page().body else {
      return;
    };
    let Some(filter) = &view.filter else {
      return;
    };
    let n = filter.options.len().max(1);
    filter.selected.set((filter.selected.get() + 1) % n);
    self.refresh(cx);
  }

  fn on_toggle_preview(&mut self, _: &TogglePreview, _: &mut Window, cx: &mut Context<Self>) {
    self.preview = !self.preview;
    cx.notify();
  }

  /// Whether the preview pane shows for the current page and selection
  pub(super) fn shows_preview(&self, cx: &App) -> bool {
    let page = self.page();
    self.preview
      && page.previews()
      && page
        .selected_row()
        .is_some_and(|r| r.command.detail(cx).is_some())
  }

  pub(super) fn click(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
    if self.page().selected == index {
      return self.run_selected(false, false, window, cx);
    }
    self.select(index, cx);
  }

  pub(super) fn click_action(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(panel) = self.panel.as_mut() {
      panel.selected = index;
    }
    self.run_panel(false, window, cx);
  }

  pub(super) fn click_confirm(&mut self, yes: bool, window: &mut Window, cx: &mut Context<Self>) {
    self.answer_confirm(yes, window, cx);
  }

  pub(super) fn toggle_dont_ask(&mut self, cx: &mut Context<Self>) {
    if let Some(confirm) = self.confirm.as_mut() {
      confirm.dont_ask = !confirm.dont_ask;
      cx.notify();
    }
  }

  pub(super) fn click_toast(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.toast = None;
    if let Some((command, action, args)) = self.retry.take() {
      self.execute(command, action, args, false, window, cx);
    }
    cx.notify();
  }

  /// Closes once the pointer moves outside the card; the first position reported, where
  /// the pointer sat when the launcher opened, does not count
  fn pointer_moved(
    &mut self,
    position: gpui_kit::Point<gpui_kit::Pixels>,
    card: gpui_kit::Size<gpui_kit::Pixels>,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let viewport = window.viewport_size();
    let origin = gpui_kit::point(
      (viewport.width - card.width) / 2.,
      (viewport.height - card.height) / 2.,
    );
    let inside = gpui_kit::Bounds::new(origin, card).contains(&position);
    if inside || !cx.config().launcher.close_on_unfocus {
      self.pointer = None;
      return;
    }
    match self.pointer {
      Some(first) if first != position => close_in(window, cx),
      Some(_) => {}
      None => self.pointer = Some(position),
    }
  }

  pub(super) fn go_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.back(window, cx);
  }

  pub(super) fn pages(&self) -> &[Page] {
    &self.pages
  }

  pub(super) fn current(&self) -> &Page {
    self.page()
  }
}

/// Closes from inside the launcher window, which cannot update itself through its handle
pub(super) fn close_in(window: &mut Window, cx: &mut App) {
  if cx.has_global::<LauncherState>() {
    cx.remove_global::<LauncherState>();
  }
  window.remove_window();
}

fn close_source(page: &Page, cx: &mut App) {
  if let Body::List(view) = &page.body {
    view.source.close(cx);
  }
}

/// The scope whose prefix `text` starts with, and the text after it
fn find_scope(scopes: &[Scope], text: &str) -> Option<(Scope, String)> {
  scopes.iter().find_map(|scope| {
    let prefix = scope.prefix.as_ref();
    let symbolic = prefix.chars().last().is_some_and(|c| !c.is_alphanumeric());
    let rest = match symbolic {
      true => text.strip_prefix(prefix)?,
      false => text.strip_prefix(prefix)?.strip_prefix(' ')?,
    };
    Some((scope.clone(), rest.to_string()))
  })
}

impl Render for Launcher {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let custom = match &self.page().body {
      Body::Custom { view, .. } => view.size,
      _ => None,
    };
    let card = window_size(self.shows_preview(cx), custom);
    let hovered = window.is_window_hovered();
    if self.was_hovered && !hovered && cx.config().launcher.close_on_unfocus {
      // not while drawing: closing removes the window
      cx.defer_in(window, |_, window, cx| close_in(window, cx));
    }
    self.was_hovered |= hovered;
    div()
      .key_context(super::CONTEXT)
      .track_focus(&self.focus)
      .capture_key_down(cx.listener(Self::on_key))
      .on_action(cx.listener(Self::on_select_next))
      .on_action(cx.listener(Self::on_select_prev))
      .on_action(cx.listener(Self::on_select_left))
      .on_action(cx.listener(Self::on_select_right))
      .on_action(cx.listener(Self::on_page_down))
      .on_action(cx.listener(Self::on_page_up))
      .on_action(cx.listener(Self::on_run))
      .on_action(cx.listener(Self::on_run_secondary))
      .on_action(cx.listener(Self::on_run_keep_open))
      .on_action(cx.listener(Self::on_submit))
      .on_action(cx.listener(Self::on_complete))
      .on_action(cx.listener(Self::on_complete_back))
      .on_action(cx.listener(Self::on_back))
      .on_action(cx.listener(Self::on_pop))
      .on_action(cx.listener(Self::on_toggle_actions))
      .on_action(cx.listener(Self::on_toggle_preview))
      .on_action(cx.listener(Self::on_cycle_filter))
      .size_full()
      .flex()
      .items_center()
      .justify_center()
      .on_mouse_down(
        MouseButton::Left,
        cx.listener(|_, _, window, cx| {
          if cx.config().launcher.close_on_unfocus {
            close_in(window, cx);
          }
        }),
      )
      .on_mouse_move(cx.listener(move |this, e: &MouseMoveEvent, window, cx| {
        this.pointer_moved(e.position, card, window, cx)
      }))
      .child(
        div()
          .w(card.width)
          .h(card.height)
          .occlude()
          .child(super::rows::frame(self, window, cx)),
      )
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn scope(prefix: &str) -> Scope {
    struct Empty;
    impl crate::command::CommandSource for Empty {
      fn id(&self) -> SharedString {
        "empty".into()
      }
      fn commands(&self, _: &Query, _: &App) -> Vec<Rc<dyn Command>> {
        Vec::new()
      }
    }
    Scope {
      prefix: prefix.to_string().into(),
      label: prefix.to_string().into(),
      placeholder: SharedString::default(),
      source: Rc::new(Empty),
    }
  }

  #[test]
  fn scopes_need_a_space_after_words() {
    let scopes = [scope("g"), scope(">"), scope("~/")];
    let found = |t| find_scope(&scopes, t).map(|(s, rest)| (s.prefix.to_string(), rest));
    assert_eq!(found("g rust"), Some(("g".into(), "rust".into())));
    assert_eq!(found("gimp"), None);
    assert_eq!(found(">ls -l"), Some((">".into(), "ls -l".into())));
    assert_eq!(found("~/Doc"), Some(("~/".into(), "Doc".into())));
    assert_eq!(found("firefox"), None);
  }
}

#[cfg(test)]
mod view_tests {
  use std::cell::Cell;

  use gpui_kit::{self as gpui, TestAppContext, VisualTestContext};

  use super::*;
  use crate::{
    command::{Meta, simple},
    view::ListView,
  };

  fn setup(cx: &mut TestAppContext) -> Rc<Cell<u32>> {
    let runs = Rc::new(Cell::new(0));
    let counter = runs.clone();
    cx.update(|cx| {
      gpui_kit::init(cx);
      cx.set_global(corona_config::Config::default());
      crate::init(cx);
      registry::register(
        simple(
          "test:count",
          |_| Meta {
            title: "Count things".into(),
            ..Default::default()
          },
          move |_| {
            let counter = counter.clone();
            vec![Action::sync("count", "Count", move |_, _, _| {
              counter.set(counter.get() + 1);
              Ok(Outcome::KeepOpen)
            })]
          },
        ),
        cx,
      );
      registry::register(
        simple(
          "test:list",
          |_| Meta {
            title: "Open list".into(),
            ..Default::default()
          },
          |_| {
            vec![Action::sync("open", "Open", |_, _, _| {
              struct Rows;
              impl crate::command::CommandSource for Rows {
                fn id(&self) -> SharedString {
                  "rows".into()
                }
                fn commands(&self, _: &Query, _: &App) -> Vec<Rc<dyn Command>> {
                  ["alpha", "beta", "gamma"]
                    .into_iter()
                    .map(|t| {
                      simple(
                        t,
                        move |_| Meta {
                          title: t.into(),
                          ..Default::default()
                        },
                        |_| Vec::new(),
                      )
                    })
                    .collect()
                }
              }
              Ok(Outcome::Push(
                ListView::new("Rows", "Search rows", Rc::new(Rows)).into_view(),
              ))
            })]
          },
        ),
        cx,
      );
    });
    runs
  }

  fn open(cx: &mut TestAppContext) -> (Entity<Launcher>, &mut VisualTestContext) {
    cx.add_window_view(|window, cx| {
      Launcher::new(
        Open::Root {
          query: String::new(),
        },
        window,
        cx,
      )
    })
  }

  #[gpui::test]
  fn typing_ranks_and_enter_runs(cx: &mut TestAppContext) {
    let runs = setup(cx);
    let (view, cx) = open(cx);
    cx.simulate_input("count");
    let titles = view.read_with(cx, |l, _| {
      l.current()
        .rows
        .iter()
        .map(|r| r.meta.title.to_string())
        .collect::<Vec<_>>()
    });
    assert_eq!(titles, ["Count things"]);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(runs.get(), 1);
  }

  #[gpui::test]
  fn push_filters_and_pops_back(cx: &mut TestAppContext) {
    setup(cx);
    let (view, cx) = open(cx);
    cx.simulate_input("open list");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let rows = view.read_with(cx, |l, _| (l.pages().len(), l.current().rows.len()));
    assert_eq!(rows, (2, 3));
    cx.simulate_input("bet");
    let first = view.read_with(cx, |l, _| l.current().rows[0].meta.title.to_string());
    assert_eq!(first, "beta");
    cx.simulate_keystrokes("ctrl-j");
    cx.simulate_keystrokes("escape");
    let (pages, query) = view.read_with(cx, |l, cx| {
      (l.pages().len(), l.input.read(cx).value().to_string())
    });
    assert_eq!(pages, 1);
    // the root comes back as it was
    assert_eq!(query, "open list");
  }

  #[gpui::test]
  fn dmenu_answers_the_choice(cx: &mut TestAppContext) {
    setup(cx);
    let (view, mut choice) = crate::dmenu::view(crate::dmenu::Options {
      lines: vec!["one".into(), "two".into(), "three".into()],
      prompt: None,
      placeholder: None,
      multi: false,
    });
    let (_, cx) = cx.add_window_view(|window, cx| Launcher::new(Open::View(view), window, cx));
    cx.simulate_input("tw");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(choice.try_recv().unwrap(), Some(vec!["two".to_string()]));
  }

  #[gpui::test]
  fn dmenu_cancel_drops_the_answer(cx: &mut TestAppContext) {
    setup(cx);
    let (view, mut choice) = crate::dmenu::view(crate::dmenu::Options {
      lines: vec!["one".into()],
      prompt: None,
      placeholder: None,
      multi: true,
    });
    let (launcher, cx) =
      cx.add_window_view(|window, cx| Launcher::new(Open::View(view), window, cx));
    drop(launcher);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    // the window is gone with its pages, so the sender is dropped
    assert!(choice.try_recv().is_err());
  }

  #[gpui::test]
  fn moving_the_pointer_outside_closes(cx: &mut TestAppContext) {
    setup(cx);
    let (_, vcx) = open(cx);
    let size = vcx.update(|window, _| window.viewport_size());
    let at = |x: f32, y: f32| gpui_kit::point(gpui_kit::px(x), gpui_kit::px(y));
    let none = gpui_kit::Modifiers::none();
    // over the card nothing happens
    vcx.simulate_mouse_move(
      gpui_kit::point(size.width / 2., size.height / 2.),
      None,
      none,
    );
    // where the pointer sat when it opened does not count
    vcx.simulate_mouse_move(at(2., 2.), None, none);
    vcx.run_until_parked();
    assert_eq!(vcx.windows().len(), 1);
    vcx.simulate_mouse_move(at(3., 2.), None, none);
    vcx.run_until_parked();
    assert!(vcx.windows().is_empty());
  }
}
