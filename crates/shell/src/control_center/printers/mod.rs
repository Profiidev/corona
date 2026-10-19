use std::path::PathBuf;

use anyhow::Result;
use corona_components::components::card::ErrorCard;
use corona_printing::{PrintingExt, ScanSettings, ScannerCaps};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  Context, IntoElement, ParentElement, Render, Styled, Subscription, Window,
  component::{
    ActiveTheme, Sizable,
    button::{Button, ButtonVariants},
  },
  div,
  prelude::FluentBuilder,
};

use crate::control_center::{ControlCenterPanel, variants::ControlCenterType};
use rust_i18n::t;

mod list;
mod scan;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tab {
  Printers,
  Scan,
}

pub struct PrintersPanel {
  tab: Tab,
  error: Option<String>,
  /// the scanner picked on the scan tab, its capabilities once loaded
  scanner: Option<String>,
  caps: Option<ScannerCaps>,
  settings: Option<ScanSettings>,
  destination: PathBuf,
  /// how many files the last scan saved
  saved: Option<usize>,
  /// the network printer a queue is being made for
  adding: Option<String>,
  _subscriptions: [Subscription; 5],
}

impl ControlCenterPanel for PrintersPanel {
  const TYPE: ControlCenterType = ControlCenterType::Printers;

  fn init(_window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    let printing = cx.printing().clone();
    // the notifier may be missing, read the current state on open
    printing.refresh();
    let subscriptions = [
      cx.observe(&printing.printers, |_, _, cx| cx.notify()),
      cx.observe(&printing.jobs, |_, _, cx| cx.notify()),
      cx.observe(&printing.scanners, |_, _, cx| cx.notify()),
      cx.observe(&printing.scan, |_, _, cx| cx.notify()),
      cx.observe(&printing.network_printers, |_, _, cx| cx.notify()),
    ];
    Self {
      tab: Tab::Printers,
      error: None,
      scanner: None,
      caps: None,
      settings: None,
      destination: corona_printing::pictures_dir(),
      saved: None,
      adding: None,
      _subscriptions: subscriptions,
    }
  }
}

impl PrintersPanel {
  fn show_error(&mut self, result: Result<()>, _: &mut Context<Self>) {
    if let Err(e) = result.log_err() {
      self.error = Some(e.to_string());
    }
  }

  fn error(&self, cx: &Context<'_, Self>) -> Option<ErrorCard> {
    let error = self.error.clone()?;
    Some(
      ErrorCard::new("printers-error-dismiss", error).on_dismiss(cx.listener(|this, _, _, cx| {
        this.error = None;
        cx.notify();
      })),
    )
  }

  fn tabs(&self, cx: &Context<'_, Self>) -> impl IntoElement {
    div().flex().gap_1().children(
      [
        (
          Tab::Printers,
          "printers-tab-printers",
          t!("app.printers.tab.printers"),
        ),
        (Tab::Scan, "printers-tab-scan", t!("app.printers.tab.scan")),
      ]
      .map(|(tab, id, label)| {
        Button::new(id)
          .label(label)
          .small()
          .flex_1()
          .cursor_pointer()
          .when(tab == self.tab, |b| b.primary())
          .on_click(cx.listener(move |this, _, _, cx| {
            this.tab = tab;
            cx.notify();
          }))
      }),
    )
  }
}

impl Render for PrintersPanel {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();
    div()
      .flex()
      .flex_col()
      .size_full()
      .gap_2()
      .child(self.tabs(cx))
      .when_some(self.error(cx), |d, error| d.child(error))
      .map(|d| match self.tab {
        Tab::Printers => d
          .child(self.printers(theme, cx))
          .when_some(self.network(theme, cx), |d, n| d.child(n))
          .child(self.jobs(theme, cx)),
        Tab::Scan => d
          .child(self.scanners(theme, cx))
          .when_some(self.scan_settings(theme, cx), |d, s| d.child(s)),
      })
  }
}

#[cfg(test)]
mod tests {
  use corona_printing::{
    ColorMode, Format, InputSource, Job, JobState, Marker, NetworkPrinter, Printer, PrinterState,
    Printing, Scanner, SourceCaps,
  };
  use gpui_kit::{
    self as gpui, AppContext, Entity, TestAppContext, WindowOptions, test::TestWindowExt,
  };

  use super::*;

  fn printer() -> Printer {
    Printer {
      name: "Office".into(),
      info: "Office laser".into(),
      location: "2nd floor".into(),
      device_uri: "ipp://10.0.0.2/ipp/print".into(),
      state: PrinterState::Idle,
      reasons: vec![],
      accepting: true,
      markers: vec![Marker {
        name: "Black".into(),
        level: Some(40),
        color: Some(0),
        kind: "toner".into(),
      }],
    }
  }

  fn job(id: i32, state: JobState) -> Job {
    Job {
      id,
      name: format!("doc {id}"),
      printer: "Office".into(),
      state,
      pages_done: Some(1),
      pages_total: Some(3),
      created: None,
    }
  }

  fn scanner(name: &str, tls: bool) -> Scanner {
    Scanner {
      name: name.into(),
      model: None,
      address: "10.0.0.5".into(),
      port: 80,
      root: "eSCL".into(),
      tls,
    }
  }

  fn network(name: &str, address: &str) -> NetworkPrinter {
    NetworkPrinter {
      name: name.into(),
      model: Some("Laser model".into()),
      address: address.into(),
      uri: format!("dnssd://{name}._ipp._tcp.local/"),
      direct_uri: format!("ipp://{address}:631/ipp/print"),
    }
  }

  fn open(cx: &mut TestAppContext) -> (gpui::AnyWindowHandle, Entity<PrintersPanel>) {
    cx.update(|cx| {
      gpui_kit::init(cx);
      cx.set_global(corona_config::Config::default());
      Printing::fake(
        cx,
        vec![printer()],
        vec![job(1, JobState::Processing), job(2, JobState::Completed)],
        vec![scanner("Brother", false), scanner("Secure", true)],
        vec![
          network("Laser Jet", "10.0.0.7"),
          network("Office", "10.0.0.2"),
        ],
      );
    });
    cx.update(|cx| {
      gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
        cx.new(|cx| PrintersPanel::init(window, cx))
      })
    })
    .unwrap()
  }

  #[gpui::test]
  fn printers_tab_shows_printers_jobs_and_ink(cx: &mut TestAppContext) {
    let (window, _) = open(cx);
    cx.update_window(window, |_, window, cx| {
      window.render_frame(cx);
      for id in [
        "printer-pause-Office",
        "printer-print-Office",
        "printer-marker-Office-0",
        "job-cancel-1",
        "printer-add-Laser Jet",
      ] {
        assert!(window.try_find(id).is_some(), "{id}");
      }
      // finished jobs cannot be canceled
      assert!(window.try_find("job-cancel-2").is_none());
      // Office has a queue at that address already
      assert!(window.try_find("printer-add-Office").is_none());
      assert!(window.try_find("scanner-Brother").is_none());
    })
    .unwrap();
  }

  #[gpui::test]
  fn switching_to_the_scan_tab(cx: &mut TestAppContext) {
    let (window, panel) = open(cx);
    cx.update_window(window, |_, window, cx| {
      window.render_frame(cx);
      for _ in 0..30 {
        if window.find("printers-tab-scan").focused() == Some(true) {
          break;
        }
        window.focus_next(cx);
        window.render_frame(cx);
      }
      window.press("space", cx);
      window.render_frame(cx);
      assert!(window.try_find("scanner-Brother").is_some());
      assert!(window.try_find("scanner-Secure").is_some());
      assert!(window.try_find("printer-pause-Office").is_none());
      // nothing picked yet, so no settings
      assert!(window.try_find("scan-start").is_none());
    })
    .unwrap();
    panel.read_with(cx, |panel, _| assert_eq!(panel.tab, Tab::Scan));

    // capabilities arrived: settings show, filtered to what the scanner offers
    panel.update(cx, |panel, cx| {
      panel.scanner = Some("Brother".into());
      panel.loaded(
        ScannerCaps {
          version: "2.0".into(),
          sources: vec![SourceCaps {
            source: InputSource::Flatbed,
            max_width: 2550,
            max_height: 3508,
          }],
          color_modes: vec![ColorMode::Color, ColorMode::Grayscale],
          formats: vec![Format::Jpeg, Format::Pdf],
          resolutions: vec![150, 300, 600],
          state: "Idle".into(),
        },
        cx,
      );
    });
    let settings = panel.read_with(cx, |p, _| p.settings.clone().unwrap());
    assert_eq!(
      (settings.resolution, settings.format, settings.source),
      (300, Format::Pdf, InputSource::Flatbed)
    );
    cx.update_window(window, |_, window, cx| {
      window.render_frame(cx);
      for id in [
        "scan-start",
        "scan-destination",
        "scan-resolution-600",
        "scan-format-jpeg",
      ] {
        assert!(window.try_find(id).is_some(), "{id}");
      }
      assert!(
        window.try_find("scan-color-monochrome").is_none(),
        "monochrome not offered"
      );
    })
    .unwrap();
  }
}
