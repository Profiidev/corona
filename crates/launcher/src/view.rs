use std::{cell::Cell, collections::HashMap, rc::Rc};

use anyhow::Result;
use gpui_kit::{AnyView, App, Pixels, SharedString, Size, Task, Window};

use crate::command::{Action, CommandSource, Outcome};

/// A view pushed on top of the root search; Esc pops it
pub enum View {
  List(ListView),
  Form(FormView),
  Detail(DetailView),
  Custom(CustomView),
}

impl View {
  pub fn title(&self) -> SharedString {
    match self {
      View::List(v) => v.title.clone(),
      View::Form(v) => v.title.clone(),
      View::Detail(v) => v.title.clone(),
      View::Custom(v) => v.title.clone(),
    }
  }
}

/// Rows of a source as a list or a grid, searched by the search bar
pub struct ListView {
  /// Breadcrumb in the search bar
  pub title: SharedString,
  pub placeholder: SharedString,
  pub source: Rc<dyn CommandSource>,
  pub layout: Layout,
  /// Has a preview pane, toggled with Ctrl+P
  pub preview: bool,
  /// Dropdown in the search bar, like "Sort: CPU"
  pub filter: Option<Filter>,
  pub empty: Option<SharedString>,
  /// Query the view opens with
  pub query: String,
}

impl ListView {
  pub fn new(
    title: impl Into<SharedString>,
    placeholder: impl Into<SharedString>,
    source: Rc<dyn CommandSource>,
  ) -> Self {
    Self {
      title: title.into(),
      placeholder: placeholder.into(),
      source,
      layout: Layout::List,
      preview: false,
      filter: None,
      empty: None,
      query: String::new(),
    }
  }

  pub fn grid(mut self, columns: usize, cell: f32, titles: bool) -> Self {
    self.layout = Layout::Grid {
      columns,
      cell,
      titles,
    };
    self
  }

  pub fn preview(mut self) -> Self {
    self.preview = true;
    self
  }

  pub fn filter(mut self, filter: Filter) -> Self {
    self.filter = Some(filter);
    self
  }

  pub fn query(mut self, query: impl Into<String>) -> Self {
    self.query = query.into();
    self
  }

  pub fn empty(mut self, text: impl Into<SharedString>) -> Self {
    self.empty = Some(text.into());
    self
  }

  pub fn into_view(self) -> View {
    View::List(self)
  }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Layout {
  List,
  Grid {
    columns: usize,
    /// Cell edge in pixels
    cell: f32,
    titles: bool,
  },
}

/// The search bar dropdown; the source reads `selected`
#[derive(Clone)]
pub struct Filter {
  pub label: SharedString,
  pub options: Vec<(SharedString, SharedString)>,
  pub selected: Rc<Cell<usize>>,
}

impl Filter {
  pub fn selected_id(&self) -> Option<&SharedString> {
    self.options.get(self.selected.get()).map(|(id, _)| id)
  }
}

pub type FormValues = HashMap<SharedString, String>;
pub type Submit = Rc<dyn Fn(FormValues, &mut Window, &mut App) -> Task<Result<Outcome>>>;
pub type Validate = Rc<dyn Fn(&str) -> Option<SharedString>>;

pub struct FormView {
  pub title: SharedString,
  pub fields: Vec<Field>,
  pub submit_label: SharedString,
  pub submit: Submit,
  /// Asked for a secret: Esc answers none through `cancel`
  pub cancel: Option<Rc<dyn Fn(&mut App)>>,
  /// Shown above the fields, like the network a password is for
  pub message: Option<SharedString>,
}

#[derive(Clone)]
pub struct Field {
  pub id: SharedString,
  pub label: SharedString,
  pub kind: FieldKind,
  pub placeholder: SharedString,
  pub value: String,
  pub required: bool,
  pub validate: Option<Validate>,
}

impl Field {
  pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>, kind: FieldKind) -> Self {
    Self {
      id: id.into(),
      label: label.into(),
      kind,
      placeholder: SharedString::default(),
      value: String::new(),
      required: false,
      validate: None,
    }
  }

  pub fn required(mut self) -> Self {
    self.required = true;
    self
  }

  pub fn value(mut self, value: impl Into<String>) -> Self {
    self.value = value.into();
    self
  }

  pub fn placeholder(mut self, text: impl Into<SharedString>) -> Self {
    self.placeholder = text.into();
    self
  }

  pub fn validate(mut self, check: impl Fn(&str) -> Option<SharedString> + 'static) -> Self {
    self.validate = Some(Rc::new(check));
    self
  }

  /// The error for `value`, required first
  pub fn error(&self, value: &str, required: SharedString) -> Option<SharedString> {
    if self.required && value.trim().is_empty() {
      return Some(required);
    }
    self.validate.as_ref().and_then(|check| check(value))
  }
}

#[derive(Clone, PartialEq)]
pub enum FieldKind {
  Text,
  Password,
  TextArea,
  /// Values by id
  Dropdown(Vec<(SharedString, SharedString)>),
  /// `true` or `false`
  Checkbox,
}

pub struct DetailView {
  pub title: SharedString,
  pub markdown: SharedString,
  /// The sidebar
  pub metadata: Vec<(SharedString, SharedString)>,
  pub actions: Vec<Action>,
}

/// A view that draws itself, like Now Playing; the host keeps Esc and back
pub struct CustomView {
  pub title: SharedString,
  /// Window size while it shows, the launcher's otherwise
  pub size: Option<Size<Pixels>>,
  /// Keeps the search field, its text goes to the view through `on_query`
  pub search: bool,
  pub build: Rc<dyn Fn(&mut Window, &mut App) -> AnyView>,
  pub on_query: Option<Rc<dyn Fn(&str, &mut App)>>,
}
