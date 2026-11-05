use std::{collections::HashSet, rc::Rc, time::Duration};

use anyhow::Result;
use corona_config::ConfigProvider;
use gpui_kit::{
  AnyView, App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement,
  IntoElement, KeyDownEvent, ListAlignment, ListState, MouseButton, MouseMoveEvent, ParentElement,
  Render, SharedString, Styled, Subscription, Task, Window,
  base::input::{InputEvent, InputState},
  div, px,
};
use rust_i18n::t;

use super::{
  Back, Complete, CompleteBack, LauncherState, Open, PageDown, PageUp, Run, RunKeepOpen,
  RunSecondary, SelectLeft, SelectNext, SelectPrev, SelectRight, Submit, ToggleActions,
  ToggleFilter, TogglePreview, form::FormPage, hud, window_size,
};
use crate::{
  args::{ArgSpec, Args, MissingArg},
  command::{
    Action, Command, Confirm, Detail, Hud, Meta, Outcome, Scope, Section, Toast, ToastKind,
  },
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
  /// The selected row's preview and primary action title, computed when the selection moves
  pub detail: Option<Detail>,
  pub primary: Option<SharedString>,
  pub list: ListState,
  /// Until the first rows arrive, so the initial selection does not preview
  pub fresh: bool,
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
      detail: None,
      primary: None,
      list: ListState::new(0, ListAlignment::Top, px(200.)),
      fresh: true,
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
  /// False when the inline arguments are invalid; only `common:*` actions may run then
  pub args_ok: bool,
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
  /// The filter dropdown is open, on this option
  pub(super) filter_menu: Option<usize>,
  pub(super) busy: usize,
  /// Bumped by reset(); results of actions started before it are dropped
  generation: u64,
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
      // every close path drops the window and with it this view
      cx.on_release(|this: &mut Self, cx| {
        for page in this.pages.drain(..).rev() {
          close_page(&page, cx);
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
      filter_menu: None,
      busy: 0,
      generation: 0,
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
    for page in self.pages.drain(..).rev() {
      close_page(&page, cx);
    }
    self.panel = None;
    self.confirm = None;
    self.toast = None;
    self.toast_timer = None;
    self.retry = None;
    self.filter_menu = None;
    self.external = false;
    self.busy = 0;
    self.generation += 1;
    match open {
      Open::Root { query } => {
        self.push_page(Page::new(Body::Root, None, query), window, cx);
      }
      Open::View(view) => {
        self.external = true;
        self.push(view, window, cx);
      }
      Open::Command { id, query, run } => {
        let command = cx.global::<Registry>().find(&id, cx);
        let parsed = command
          .as_ref()
          .filter(|_| run)
          .map(|c| parse_inline(&c.meta(cx), &query));
        // required arguments left out: behave like a plain open and let the user type them
        let prefill = !run
          || command
            .as_ref()
            .is_some_and(|c| needs_args_row(&c.meta(cx)))
          || matches!(&parsed, Some(Err(e)) if e.is::<MissingArg>());
        let root_query = match &command {
          Some(command) if prefill => format!("{} {query}", command.meta(cx).title)
            .trim()
            .to_string(),
          _ => String::new(),
        };
        self.push_page(Page::new(Body::Root, None, root_query), window, cx);
        match command {
          Some(_) if prefill => {}
          Some(command) => {
            self.external = true;
            match parsed.expect("run parses the query") {
              Ok(args) => {
                if let Some(action) = command.actions(cx).into_iter().next() {
                  self.run(Some(command), action, args, false, window, cx);
                }
              }
              Err(e) => self.show_toast(
                Toast {
                  kind: ToastKind::Failure,
                  title: t!("app.launcher.failed").into(),
                  message: Some(format!("{e:#}").into()),
                  action: None,
                },
                cx,
              ),
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
    // only the page that owns the subscription refreshes; pop() refreshes the one below
    let idx = self.pages.len();
    let notify: Rc<dyn Fn(&mut App)> = Rc::new(move |cx| {
      let _ = weak.update(cx, |this, cx| {
        if this.pages.len() == idx + 1 {
          this.refresh(cx)
        }
      });
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
      input.set_value(query.clone(), window, cx);
      input.set_placeholder(placeholder, window, cx);
    });
    // set_value emits no change event, so a prefilled query would never become a scope
    self.detect_scope(&query, window, cx);
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
    close_page(&page, cx);
    let (query, placeholder) = (self.page().query.clone(), self.page().placeholder());
    self.input.update(cx, |input, cx| {
      input.set_value(query, window, cx);
      // selected so the next backspace clears the restored query
      input.select_all(window, cx);
      input.set_placeholder(placeholder, window, cx);
    });
    self.refresh(cx);
    self.focus(window, cx);
  }

  /// Opened into a command: the root page under it never shows
  fn root_hidden(&self) -> bool {
    self.external && matches!(self.pages.first().map(|p| &p.body), Some(Body::Root))
  }

  /// Turns a `prefix ` at the start of the root query into the scope chip
  fn detect_scope(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
    let page = self.page();
    if !matches!(page.body, Body::Root) || page.scope.is_some() {
      return;
    }
    let scopes = cx.global::<Registry>().scopes(cx);
    if let Some((scope, rest)) = find_scope(&scopes, text) {
      let placeholder = scope.placeholder.clone();
      self.page_mut().scope = Some(scope);
      self.page_mut().query = rest.clone();
      self.input.update(cx, |input, cx| {
        input.set_value(rest, window, cx);
        input.set_placeholder(placeholder, window, cx);
      });
    }
  }

  fn on_query(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let text = self.input.read(cx).value().to_string();
    let page = self.page_mut();
    page.query = text.clone();
    page.args = None;
    page.completion = 0;
    self.detect_scope(&text, window, cx);
    if let Body::Custom { view, .. } = &self.page().body
      && let Some(on_query) = view.on_query.clone()
    {
      on_query(&text, cx);
    }
    self.refresh(cx);
  }

  /// Recomputes the rows of the current page, keeping the selection on the same id
  pub(super) fn refresh(&mut self, cx: &mut Context<Self>) {
    if self.filter().is_none() {
      self.filter_menu = None;
    }
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
    // a tick-only refresh must not reset the list and snap the scroll back
    let changed = page.rows.len() != rows.len()
      || page.items.len() != items.len()
      || page.rows.iter().zip(&rows).any(|(a, b)| a.id() != b.id());
    page.rows = rows;
    page.items = items;
    if changed {
      page.list.reset(count);
    }
    self.select_at(selected, changed, cx);
  }

  pub(super) fn select(&mut self, index: usize, cx: &mut Context<Self>) {
    self.select_at(index, true, cx);
  }

  fn select_at(&mut self, index: usize, reveal: bool, cx: &mut Context<Self>) {
    let page = self.page_mut();
    page.selected = index.min(page.rows.len().saturating_sub(1));
    let before = page.selected_id.take();
    page.selected_id = page.selected_row().map(Row::id);
    let moved = page.selected_id != before;
    // the first selection after opening a page is not a user choice
    let initial = page.fresh && page.selected_id.is_some();
    page.fresh &= page.selected_id.is_none();
    if moved
      && !initial
      && let Some(command) = page.selected_row().map(|r| r.command.clone())
    {
      command.on_select(cx);
    }
    let row = self.page().selected_row().cloned();
    let detail = row
      .as_ref()
      .filter(|_| self.preview && self.page().previews())
      .and_then(|r| r.command.detail(cx));
    let primary = row
      .as_ref()
      .filter(|r| self.inline_args(r).is_ok())
      .and_then(|r| r.command.actions(cx).into_iter().next())
      .map(|a| a.title);
    let page = self.page_mut();
    page.detail = detail;
    page.primary = primary;
    let item = match page.layout() {
      Layout::Grid { columns, .. } => page.selected / columns.max(1),
      Layout::List => page
        .items
        .iter()
        .position(|i| matches!(i, Item::Row(r) if *r == page.selected))
        .unwrap_or(0),
    };
    // reveal the section header above the first row too
    let reveal_item = if page.selected == 0 { 0 } else { item };
    if reveal || moved {
      page.list.scroll_to_reveal_item(reveal_item);
    }
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
    parse_inline(&row.meta, row.found.args.as_deref().unwrap_or(""))
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
        Err((index, e)) => {
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
    if needs_args_row(&row.meta) {
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
    let specs: Vec<ArgSpec> = row.meta.args.clone();
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
      && !(confirm.skippable && self.skip_confirm.contains(action.id.as_ref()))
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
    // a second Enter must not rerun a slow action
    if self.busy > 0 && !(keep_open || action.keep_open) {
      return;
    }
    self.panel = None;
    // only root commands rank by use; rows of pushed views and scopes come and go
    if let Some(command) = &command
      && matches!(self.page().body, Body::Root)
      && self.page().scope.is_none()
      && !action.id.starts_with("common:")
      && command.meta(cx).section != Section::Running
      && cx.global::<Registry>().find(&command.id(), cx).is_some()
    {
      registry::record_use(&command.id(), cx);
    }
    let task = (action.run)(&args, window, cx);
    let keep_open = keep_open || action.keep_open;
    self.busy += 1;
    let generation = self.generation;
    let depth = self.pages.len();
    cx.notify();
    cx.spawn_in(window, async move |this, cx| {
      let mut result = task.await;
      let failure = result.as_ref().err().map(|e| format!("{e:#}"));
      let id = action.id.clone();
      let updated = this.update_in(cx, |this, window, cx| {
        if this.generation != generation {
          tracing::debug!("launcher action {id} finished after a reset, result dropped");
          return;
        }
        this.busy -= 1;
        // the user left the page that started the action: Push/Pop would hit another page
        if this.pages.len() != depth && matches!(result, Ok(Outcome::Push(_) | Outcome::Pop)) {
          tracing::debug!("launcher action {id} finished on another page, navigation dropped");
          result = Ok(Outcome::KeepOpen);
        }
        this.finish(result, command, action, args, keep_open, window, cx);
      });
      if updated.is_err() {
        // window already closed, finish() never ran
        match failure {
          Some(e) => tracing::warn!("launcher action {id} failed after the window closed: {e}"),
          None => tracing::debug!("launcher action {id} finished after the window closed"),
        }
      }
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
        self.show_toast(
          Toast {
            kind: ToastKind::Failure,
            title: t!("app.launcher.failed").into(),
            message: Some(format!("{e:#}").into()),
            action: Some(retry),
          },
          cx,
        );
        self.retry = Some((command, action, args));
        return;
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
        let copied = paste::copy(&clip, cx);
        let hud = Hud::new(if copied {
          t!("app.launcher.hud.copied")
        } else {
          t!("app.launcher.hud.copy_failed")
        });
        if keep_open {
          self.show_toast(
            Toast {
              kind: if copied {
                ToastKind::Success
              } else {
                ToastKind::Failure
              },
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
    let sticky = toast.action.is_some();
    self.retry = None;
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

  /// The row's own actions, plus favorite, alias, hide and disable for root commands
  fn row_actions(&self, row: &Row, cx: &App) -> Vec<Action> {
    let mut actions = row.command.actions(cx);
    let page = self.page();
    if matches!(page.body, Body::Root) && page.scope.is_none() && row.managed {
      actions.extend(crate::common::actions(row.id(), cx));
    }
    actions
  }

  fn open_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let (command, args, args_ok, actions) = match &self.page().body {
      Body::Detail(detail) => (None, Args::default(), true, detail.actions.clone()),
      _ => {
        let Some(row) = self.page().selected_row().cloned() else {
          return;
        };
        let parsed = self.inline_args(&row);
        let args_ok = parsed.is_ok();
        let actions = self.row_actions(&row, cx);
        (
          Some(row.command),
          parsed.unwrap_or_default(),
          args_ok,
          actions,
        )
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
      args_ok,
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
    if !panel.args_ok && !action.id.starts_with("common:") {
      return;
    }
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
        let spec = &row.meta.args[arg_index(typed, row.meta.args.len())];
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
    if self.filter_menu.take().is_some() {
      return cx.notify();
    }
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
    let in_search = self.input.read(cx).focus_handle(cx).is_focused(window);
    if e.keystroke.key == "backspace" && in_search && self.input.read(cx).value().is_empty() {
      if self.page_mut().scope.take().is_some() {
        cx.stop_propagation();
        let placeholder = self.page().placeholder();
        self.input.update(cx, |input, cx| {
          input.set_placeholder(placeholder, window, cx)
        });
        self.page_mut().query = String::new();
        self.refresh(cx);
      } else if self.pages.len() > 1 && self.panel.is_none() && self.confirm.is_none() {
        // back to the view before, like Esc
        cx.stop_propagation();
        self.pop(window, cx);
      }
      return;
    }
    let modified = e.keystroke.modifiers.control || e.keystroke.modifiers.alt;
    if !modified || self.panel.is_some() || self.confirm.is_some() {
      return;
    }
    let keys = e.keystroke.unparse(cx.platform_info().os);
    let Some(row) = self.page().selected_row().cloned() else {
      return;
    };
    if let Some(action) = self
      .row_actions(&row, cx)
      .into_iter()
      .skip(1)
      .find(|a| a.shortcut.as_deref() == Some(keys.as_str()))
    {
      let args = match self.inline_args(&row) {
        Ok(args) => args,
        Err(_) if action.id.starts_with("common:") => Args::default(),
        Err(_) => return,
      };
      cx.stop_propagation();
      self.run(Some(row.command), action, args, false, window, cx);
    }
  }

  fn on_select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
    if self.filter_menu.is_some() {
      return self.move_filter(1, cx);
    }
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
    if self.filter_menu.is_some() {
      return self.move_filter(-1, cx);
    }
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
    if let Some(at) = self.filter_menu {
      return self.pick_filter(at, cx);
    }
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

  fn on_toggle_actions(&mut self, _: &ToggleActions, window: &mut Window, cx: &mut Context<Self>) {
    if self.panel.take().is_some() {
      self.focus(window, cx);
      return cx.notify();
    }
    self.open_panel(window, cx);
  }

  fn on_toggle_filter(&mut self, _: &ToggleFilter, _: &mut Window, cx: &mut Context<Self>) {
    self.toggle_filter(cx);
  }

  /// The current view's dropdown
  pub(super) fn filter(&self) -> Option<&crate::view::Filter> {
    match &self.page().body {
      Body::List(view) => view.filter.as_ref(),
      _ => None,
    }
  }

  /// Opens the dropdown on the current option, or closes it
  pub(super) fn toggle_filter(&mut self, cx: &mut Context<Self>) {
    self.filter_menu = match (self.filter_menu, self.filter()) {
      (None, Some(filter)) => Some(filter.selected.get()),
      _ => None,
    };
    cx.notify();
  }

  pub(super) fn pick_filter(&mut self, index: usize, cx: &mut Context<Self>) {
    if let Some(filter) = self.filter() {
      filter
        .selected
        .set(index.min(filter.options.len().saturating_sub(1)));
    }
    self.filter_menu = None;
    self.refresh(cx);
  }

  fn move_filter(&mut self, delta: isize, cx: &mut Context<Self>) {
    let n = self.filter().map_or(0, |f| f.options.len()) as isize;
    if let Some(at) = self.filter_menu.as_mut() {
      *at = (*at as isize + delta).clamp(0, (n - 1).max(0)) as usize;
      cx.notify();
    }
  }

  fn on_toggle_preview(&mut self, _: &TogglePreview, _: &mut Window, cx: &mut Context<Self>) {
    self.preview = !self.preview;
    self.select(self.page().selected, cx);
    cx.notify();
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
    let toast = self.toast.take();
    match self.retry.take() {
      Some((command, action, args)) => self.execute(command, action, args, false, window, cx),
      None => {
        if let Some(action) = toast.and_then(|t| t.action) {
          self.execute(None, action, Args::default(), false, window, cx);
        }
      }
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

/// Lets go of what the page holds: its list source, or its form, which is cancelled
fn close_page(page: &Page, cx: &mut App) {
  match &page.body {
    Body::List(view) => view.source.close(cx),
    Body::Form(form) => {
      if let Some(cancel) = form.read(cx).cancel.clone() {
        cancel(cx);
      }
    }
    _ => {}
  }
}

/// Whether running the command needs the arguments row filled in first
fn needs_args_row(meta: &Meta) -> bool {
  meta.args_row && meta.args.iter().any(|a| !a.optional)
}

/// Parses what was typed after a command; an args row takes the whole tail as its first argument
fn parse_inline(meta: &Meta, typed: &str) -> Result<Args> {
  if meta.args.is_empty() || (meta.args_row && typed.trim().is_empty()) {
    return Ok(Args::default());
  }
  let specs = match meta.args_row {
    true => &meta.args[..1],
    false => &meta.args[..],
  };
  Args::parse(specs, typed)
}

/// Index of the argument spec for the word being typed, capped at the last spec
fn arg_index(typed: &str, specs: usize) -> usize {
  let words = typed.split_whitespace().count();
  let idx = if typed.ends_with(char::is_whitespace) {
    words
  } else {
    words.saturating_sub(1)
  };
  idx.min(specs.saturating_sub(1))
}

/// The scope whose prefix `text` starts with, and the text after it
fn find_scope(scopes: &[Scope], text: &str) -> Option<(Scope, String)> {
  scopes.iter().find_map(|scope| {
    let prefix = scope.prefix.as_ref();
    if prefix.is_empty() {
      return None;
    }
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
    let card = window_size(cx);
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
      .on_action(cx.listener(Self::on_toggle_actions))
      .on_action(cx.listener(Self::on_toggle_preview))
      .on_action(cx.listener(Self::on_toggle_filter))
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
  fn args_row_takes_whole_tail() {
    let meta = Meta {
      args: vec![ArgSpec::text("text"), ArgSpec::text("target")],
      args_row: true,
      ..Default::default()
    };
    assert_eq!(
      parse_inline(&meta, "hello world").unwrap().text(0),
      Some("hello world")
    );
    assert!(parse_inline(&meta, " ").unwrap().text(0).is_none());
    assert!(needs_args_row(&meta));
    let optional = Meta {
      args: vec![ArgSpec::text("text").optional()],
      ..meta.clone()
    };
    assert!(!needs_args_row(&optional));
    let plain = Meta {
      args_row: false,
      ..meta
    };
    assert_eq!(
      parse_inline(&plain, "hello world").unwrap().text(1),
      Some("world")
    );
  }

  #[test]
  fn arg_index_follows_typed_words() {
    assert_eq!(arg_index("", 2), 0);
    assert_eq!(arg_index("a", 2), 0);
    assert_eq!(arg_index("a ", 2), 1);
    assert_eq!(arg_index("a b", 3), 1);
    assert_eq!(arg_index("a b c d", 2), 1);
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
    // selected, so one backspace clears it
    cx.simulate_keystrokes("backspace");
    assert_eq!(
      view.read_with(cx, |l, cx| l.input.read(cx).value().to_string()),
      ""
    );
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

  #[gpui::test]
  fn the_filter_is_a_dropdown(cx: &mut TestAppContext) {
    setup(cx);
    let selected = Rc::new(std::cell::Cell::new(0));
    let filter = crate::view::Filter {
      label: "Type".into(),
      options: vec![
        ("a".into(), "A".into()),
        ("b".into(), "B".into()),
        ("c".into(), "C".into()),
      ],
      selected: selected.clone(),
    };
    let source = crate::kit::ListSource::new("x", |_, _| Vec::new());
    let view = ListView::new("X", "x", Rc::new(source))
      .filter(filter)
      .into_view();
    let (launcher, cx) =
      cx.add_window_view(|window, cx| Launcher::new(Open::View(view), window, cx));
    cx.simulate_keystrokes("ctrl-f");
    assert_eq!(launcher.read_with(cx, |l, _| l.filter_menu), Some(0));
    cx.simulate_keystrokes("down down enter");
    assert_eq!(selected.get(), 2);
    assert_eq!(launcher.read_with(cx, |l, _| l.filter_menu), None);
    // Esc closes the menu, not the launcher
    cx.simulate_keystrokes("ctrl-f escape");
    assert_eq!(launcher.read_with(cx, |l, _| l.filter_menu), None);
    assert_eq!(cx.windows().len(), 1);
  }

  #[gpui::test]
  fn backspace_on_an_empty_search_pops(cx: &mut TestAppContext) {
    setup(cx);
    let (view, cx) = open(cx);
    cx.simulate_input("open list");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("b");
    // the first backspace deletes the text, the next one goes back
    cx.simulate_keystrokes("backspace");
    assert_eq!(view.read_with(cx, |l, _| l.pages().len()), 2);
    cx.simulate_keystrokes("backspace");
    assert_eq!(view.read_with(cx, |l, _| l.pages().len()), 1);
    // on the root it only edits
    cx.simulate_keystrokes("backspace");
    assert_eq!(cx.windows().len(), 1);
  }
}
