use std::{borrow::Cow, path::PathBuf};

use anyhow::{Context as _, Result};
use corona_components::async_listener::AsyncListenerExt;
use corona_components::components::card::CardExt;
use corona_printing::{
  ColorMode, Format, InputSource, PrintingExt, ScanSettings, Scanner, ScannerCaps,
};
use gpui_kit::{
  Context, IntoElement, ParentElement, PathPromptOptions, Styled,
  assets::IconName,
  base::{Disableable, StyledExt},
  component::{
    ActiveTheme, Icon, Sizable, Theme,
    button::{Button, ButtonVariants},
    scroll::ScrollableElement,
  },
  div,
  prelude::FluentBuilder,
  px,
};

use crate::control_center::printers::{PrintersPanel, list::placeholder};
use rust_i18n::t;

/// What a fresh scan uses: 300 dpi or the closest, PDF when offered.
fn defaults(caps: &ScannerCaps, destination: PathBuf) -> ScanSettings {
  ScanSettings {
    resolution: caps
      .resolutions
      .iter()
      .copied()
      .min_by_key(|dpi| dpi.abs_diff(300))
      .unwrap_or(300),
    color_mode: caps
      .color_modes
      .first()
      .copied()
      .unwrap_or(ColorMode::Color),
    source: caps
      .sources
      .first()
      .map_or(InputSource::Flatbed, |s| s.source),
    format: match caps.formats.contains(&Format::Pdf) {
      true => Format::Pdf,
      false => caps.formats.first().copied().unwrap_or(Format::Jpeg),
    },
    destination,
  }
}

fn color_label(mode: ColorMode) -> (&'static str, Cow<'static, str>) {
  match mode {
    ColorMode::Color => ("color", t!("app.printers.color.color")),
    ColorMode::Grayscale => ("grayscale", t!("app.printers.color.grayscale")),
    ColorMode::Monochrome => ("monochrome", t!("app.printers.color.monochrome")),
  }
}

fn source_label(source: InputSource) -> (&'static str, Cow<'static, str>) {
  match source {
    InputSource::Flatbed => ("flatbed", t!("app.printers.sources.flatbed")),
    InputSource::Adf => ("adf", t!("app.printers.sources.adf")),
  }
}

fn format_label(format: Format) -> (&'static str, Cow<'static, str>) {
  match format {
    Format::Pdf => ("pdf", "PDF".into()),
    Format::Png => ("png", "PNG".into()),
    Format::Jpeg => ("jpeg", "JPEG".into()),
  }
}

/// Choice options: each value with its id and label.
fn labeled<T>(
  values: impl Iterator<Item = T>,
  pick: fn(T) -> (&'static str, Cow<'static, str>),
) -> Vec<(T, String, Cow<'static, str>)>
where
  T: Copy,
{
  values
    .map(|value| {
      let (id, label) = pick(value);
      (value, id.to_string(), label)
    })
    .collect()
}

impl PrintersPanel {
  /// The capabilities of the picked scanner arrived.
  pub(super) fn loaded(&mut self, caps: ScannerCaps, cx: &mut Context<Self>) {
    self.settings = Some(defaults(&caps, self.destination.clone()));
    self.caps = Some(caps);
    cx.notify();
  }

  pub(super) fn scanners(&self, theme: &Theme, cx: &Context<'_, Self>) -> impl IntoElement {
    let scanners = cx.printing().list_scanners(cx);
    div()
      .flex()
      .flex_col()
      .w_full()
      .when_else(
        scanners.len() > 2,
        |d| d.min_h(px(96.)),
        |d| d.flex_shrink_0(),
      )
      .gap_2()
      .p_2()
      .card(cx)
      .child(
        div()
          .font_bold()
          .text_sm()
          .child(t!("app.printers.scanners")),
      )
      .when(scanners.is_empty(), |d| {
        d.child(placeholder(theme, t!("app.printers.no_scanners")))
      })
      .child(
        div()
          .flex()
          .flex_col()
          .gap_1()
          .h_auto()
          .overflow_y_scrollbar()
          .id("scanners-list")
          .children(scanners.iter().map(|s| self.scanner_row(theme, s, cx))),
      )
      .when(!scanners.iter().any(|s| s.sane.is_some()), |d| {
        d.child(
          div()
            .text_xs()
            .text_color(theme.colors.muted_foreground)
            .child(t!("app.printers.sane_hint")),
        )
      })
  }

  fn scanner_row(
    &self,
    theme: &Theme,
    scanner: &Scanner,
    cx: &Context<'_, Self>,
  ) -> impl IntoElement {
    let selected = self.scanner.as_deref() == Some(&scanner.name);
    let mut details: Vec<_> = scanner.model.clone().into_iter().collect();
    if cx
      .printing()
      .scan_progress(cx)
      .is_some_and(|p| p.scanner == scanner.name)
    {
      details.push(t!("app.printers.scanning_start").to_string());
    }
    if scanner.tls {
      details.push(t!("app.printers.tls").to_string());
    }
    let details = details.join(" · ");
    let name = scanner.name.clone();

    div()
      .flex()
      .w_full()
      .gap_2()
      .p_2()
      .items_center()
      .rounded_xl()
      .bg(theme.colors.background)
      .child(
        Icon::new(IconName::ScanLine)
          .small()
          .when(selected, |i| i.text_color(theme.colors.primary)),
      )
      .child(
        div()
          .flex()
          .flex_col()
          .flex_1()
          .min_w_0()
          .child(div().text_sm().truncate().child(scanner.name.clone()))
          .child(
            div()
              .text_xs()
              .text_color(theme.colors.muted_foreground)
              .truncate()
              .child(details),
          ),
      )
      .child(
        Button::new(format!("scanner-{name}"))
          .icon(IconName::Check)
          .small()
          .cursor_pointer()
          .when(selected, |b| b.primary())
          .loading(selected && self.caps.is_none())
          .on_click(cx.async_listener(
            move |this, _, _, cx| {
              this.scanner = Some(name.clone());
              this.error = None;
              (this.caps, this.settings, this.saved) = (None, None, None);
              let caps = cx.printing().scanner_capabilities(&name, cx);
              let name = name.clone();
              async move { (name, caps.await) }
            },
            |this, (name, caps): (String, Result<ScannerCaps>), cx| {
              // a later pick wins over a slow answer
              if this.scanner.as_deref() != Some(&name) {
                return;
              }
              match caps {
                Ok(caps) => this.loaded(caps, cx),
                Err(e) => {
                  this.scanner = None;
                  this.show_error(Err(e), cx);
                }
              }
            },
          )),
      )
  }

  /// One row of buttons, one per value the scanner offers.
  fn choice<T: Copy + PartialEq + 'static>(
    &self,
    key: &'static str,
    label: Cow<'static, str>,
    options: Vec<(T, String, Cow<'static, str>)>,
    current: T,
    set: fn(&mut ScanSettings, T),
    cx: &Context<'_, Self>,
  ) -> impl IntoElement {
    let theme = cx.theme();
    div()
      .flex()
      .flex_col()
      .gap_1()
      .child(
        div()
          .text_xs()
          .text_color(theme.colors.muted_foreground)
          .child(label),
      )
      .child(
        div()
          .flex()
          .flex_wrap()
          .gap_1()
          .children(options.into_iter().map(|(value, id, label)| {
            Button::new(format!("scan-{key}-{id}"))
              .label(label)
              .small()
              .cursor_pointer()
              .when(value == current, |b| b.primary())
              .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(settings) = this.settings.as_mut() {
                  set(settings, value);
                }
                cx.notify();
              }))
          })),
      )
  }

  pub(super) fn scan_settings(
    &self,
    theme: &Theme,
    cx: &Context<'_, Self>,
  ) -> Option<impl IntoElement> {
    let (caps, settings) = (self.caps.as_ref()?, self.settings.as_ref()?);
    // one scan at a time, on any scanner
    let busy = cx.printing().scan_progress(cx);
    let progress = busy.filter(|p| self.scanner.as_ref() == Some(&p.scanner));
    let status = match (busy, progress, self.saved) {
      // a flatbed or a PDF reports no page before the end
      (_, Some(p), _) if p.pages == 0 => Some(t!("app.printers.scanning_start")),
      (_, Some(p), _) if p.pages == 1 => Some(t!("app.printers.scanning_one")),
      (_, Some(p), _) => Some(t!("app.printers.scanning", pages = p.pages)),
      (Some(p), None, _) => Some(t!("app.printers.scanning_other", scanner = p.scanner)),
      (None, _, Some(1)) => Some(t!("app.printers.saved_one")),
      (None, _, Some(count)) => Some(t!("app.printers.saved", count = count)),
      _ => None,
    };
    let dpi: Vec<_> = caps
      .resolutions
      .iter()
      .map(|dpi| (*dpi, dpi.to_string(), t!("app.printers.dpi", dpi = dpi)))
      .collect();

    Some(
      div()
        .flex()
        .flex_col()
        .w_full()
        .min_h(px(160.))
        .gap_2()
        .p_2()
        .card(cx)
        .child(
          div()
            .flex()
            .flex_col()
            .gap_2()
            .h_auto()
            .overflow_y_scrollbar()
            .id("scan-options")
            .child(self.choice(
              "resolution",
              t!("app.printers.resolution"),
              dpi,
              settings.resolution,
              |s, v| s.resolution = v,
              cx,
            ))
            .child(self.choice(
              "color",
              t!("app.printers.color_mode"),
              labeled(caps.color_modes.iter().copied(), color_label),
              settings.color_mode,
              |s, v| s.color_mode = v,
              cx,
            ))
            .when(caps.sources.len() > 1, |d| {
              d.child(self.choice(
                "source",
                t!("app.printers.source"),
                labeled(caps.sources.iter().map(|s| s.source), source_label),
                settings.source,
                |s, v| s.source = v,
                cx,
              ))
            })
            .child(self.choice(
              "format",
              t!("app.printers.format"),
              labeled(caps.formats.iter().copied(), format_label),
              settings.format,
              |s, v| s.format = v,
              cx,
            )),
        )
        .child(
          div()
            .flex()
            .gap_2()
            .items_center()
            .child(
              div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                  div()
                    .text_xs()
                    .text_color(theme.colors.muted_foreground)
                    .child(t!("app.printers.destination")),
                )
                .child(
                  div()
                    .text_sm()
                    .truncate()
                    .child(settings.destination.display().to_string()),
                ),
            )
            .child(
              Button::new("scan-destination")
                .icon(IconName::FolderOpen)
                .small()
                .cursor_pointer()
                .tooltip(t!("app.printers.choose_folder"))
                .on_click(cx.async_listener(
                  |this, _, _, cx| {
                    this.error = None;
                    let paths = cx.prompt_for_paths(PathPromptOptions {
                      files: false,
                      directories: true,
                      multiple: false,
                      prompt: Some(t!("app.printers.choose_folder").into()),
                    });
                    async move {
                      let picked = paths.await.context("folder picker closed")??;
                      Ok(picked.and_then(|p| p.into_iter().next()))
                    }
                  },
                  |this, picked: Result<Option<PathBuf>>, cx| match picked {
                    Ok(Some(dir)) => {
                      if let Some(settings) = this.settings.as_mut() {
                        settings.destination = dir.clone();
                      }
                      this.destination = dir;
                    }
                    Ok(None) => {}
                    Err(e) => this.show_error(Err(e), cx),
                  },
                )),
            ),
        )
        .child(
          div()
            .flex()
            .gap_2()
            .items_center()
            .child(
              div()
                .flex_1()
                .min_w_0()
                .text_xs()
                .text_color(theme.colors.muted_foreground)
                .truncate()
                .when_some(status, |d, s| d.child(s)),
            )
            .child(
              Button::new("scan-start")
                .icon(IconName::ScanLine)
                .label(t!("app.printers.scan"))
                .small()
                .primary()
                .cursor_pointer()
                .loading(progress.is_some())
                .disabled(busy.is_some())
                .on_click(cx.async_listener(
                  |this, _, _, cx| {
                    (this.saved, this.error) = (None, None);
                    let printing = cx.printing().clone();
                    let name = this.scanner.clone();
                    let task = match (&name, this.settings.clone()) {
                      (Some(name), Some(settings)) => Some(printing.scan(name, settings, cx)),
                      _ => None,
                    };
                    async move {
                      let files = match task {
                        Some(task) => task.await,
                        None => Ok(Vec::new()),
                      };
                      (name, files)
                    }
                  },
                  |this, (name, files): (Option<String>, Result<Vec<PathBuf>>), cx| {
                    // another scanner was picked meanwhile, its card is not about this scan
                    if this.scanner != name {
                      return;
                    }
                    match files {
                      Ok(files) => this.saved = Some(files.len()),
                      Err(e) => this.show_error(Err(e), cx),
                    }
                  },
                )),
            ),
        ),
    )
  }
}

#[cfg(test)]
mod tests {
  use corona_printing::SourceCaps;

  use super::*;

  #[test]
  fn defaults_pick_300_dpi_and_pdf() {
    let mut caps = ScannerCaps {
      sources: vec![SourceCaps {
        source: InputSource::Adf,
        max_width: 1,
        max_height: 1,
      }],
      color_modes: vec![ColorMode::Grayscale],
      formats: vec![Format::Jpeg, Format::Pdf],
      resolutions: vec![75, 200, 600],
      ..Default::default()
    };
    let s = defaults(&caps, "/tmp".into());
    assert_eq!(
      (s.resolution, s.color_mode, s.source, s.format),
      (200, ColorMode::Grayscale, InputSource::Adf, Format::Pdf)
    );
    caps = ScannerCaps::default();
    let s = defaults(&caps, "/tmp".into());
    assert_eq!(
      (s.resolution, s.color_mode, s.source, s.format),
      (300, ColorMode::Color, InputSource::Flatbed, Format::Jpeg)
    );
  }
}
