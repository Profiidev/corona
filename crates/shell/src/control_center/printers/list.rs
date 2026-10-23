use std::borrow::Cow;

use anyhow::Context as _;
use corona_components::async_listener::AsyncListenerExt;
use corona_components::components::card::CardExt;
use corona_printing::{Job, JobState, Printer, PrinterState, PrintingExt};
use gpui_kit::{
  Context, IntoElement, ParentElement, PathPromptOptions, Styled,
  assets::IconName,
  base::{Disableable, StyledExt},
  component::{
    Icon, Sizable, Theme,
    button::{Button, ButtonVariant, ButtonVariants},
    progress::Progress,
    scroll::ScrollableElement,
    switch::Switch,
  },
  div,
  prelude::FluentBuilder,
  px, rgb,
};

use crate::control_center::printers::PrintersPanel;
use rust_i18n::t;

fn printer_state(state: PrinterState) -> Cow<'static, str> {
  match state {
    PrinterState::Idle => t!("app.printers.state.idle"),
    PrinterState::Processing => t!("app.printers.state.processing"),
    PrinterState::Stopped => t!("app.printers.state.stopped"),
  }
}

fn job_state(state: JobState) -> Cow<'static, str> {
  match state {
    JobState::Pending => t!("app.printers.job_state.pending"),
    JobState::Held => t!("app.printers.job_state.held"),
    JobState::Processing => t!("app.printers.job_state.processing"),
    JobState::Stopped => t!("app.printers.job_state.stopped"),
    JobState::Canceled => t!("app.printers.job_state.canceled"),
    JobState::Aborted => t!("app.printers.job_state.aborted"),
    JobState::Completed => t!("app.printers.job_state.completed"),
  }
}

pub(super) fn placeholder(theme: &Theme, text: impl IntoElement) -> impl IntoElement {
  div()
    .flex()
    .justify_center()
    .p_2()
    .text_xs()
    .text_color(theme.colors.muted_foreground)
    .child(text)
}

impl PrintersPanel {
  pub(super) fn printers(&self, theme: &Theme, cx: &Context<'_, Self>) -> impl IntoElement {
    let printers = cx.printing().list_printers(cx);
    div()
      .flex()
      .flex_col()
      .w_full()
      .when_else(
        printers.len() > 1,
        |d| d.min_h(px(128.)),
        |d| d.flex_shrink_0(),
      )
      .gap_2()
      .p_2()
      .card(cx)
      .child(
        div()
          .font_bold()
          .text_sm()
          .child(t!("app.printers.tab.printers")),
      )
      .when(printers.is_empty(), |d| {
        d.child(placeholder(theme, t!("app.printers.no_printers")))
      })
      .child(
        div()
          .flex()
          .flex_col()
          .gap_1()
          .h_auto()
          .overflow_y_scrollbar()
          .id("printers-list")
          .children(printers.iter().map(|p| self.printer(theme, p, cx))),
      )
  }

  fn printer(&self, theme: &Theme, printer: &Printer, cx: &Context<'_, Self>) -> impl IntoElement {
    let name = printer.name.clone();
    let state = match printer.offline {
      true => t!("app.printers.offline"),
      false => printer_state(printer.state),
    };
    let mut details = vec![state.to_string()];
    if !printer.accepting {
      details.push(t!("app.printers.not_accepting").to_string());
    }
    if !printer.location.is_empty() {
      details.push(printer.location.clone());
    }
    let title = match printer.info.is_empty() {
      true => printer.name.clone(),
      false => printer.info.clone(),
    };

    div()
      .flex()
      .flex_col()
      .w_full()
      .gap_2()
      .p_2()
      .rounded_xl()
      .bg(theme.colors.background)
      .child(
        div()
          .flex()
          .gap_2()
          .items_center()
          .child(
            Icon::new(IconName::Printer)
              .small()
              .when(printer.state == PrinterState::Processing, |i| {
                i.text_color(theme.colors.primary)
              })
              .when(
                printer.state == PrinterState::Stopped || printer.offline,
                |i| i.text_color(theme.colors.danger),
              ),
          )
          .child(
            div()
              .flex()
              .flex_col()
              .flex_1()
              .min_w_0()
              .child(div().text_sm().truncate().child(title))
              .child(
                div()
                  .text_xs()
                  .text_color(theme.colors.muted_foreground)
                  .truncate()
                  .child(details.join(" · ")),
              ),
          )
          .child(
            Button::new(format!("printer-print-{name}"))
              .icon(IconName::FileText)
              .small()
              .cursor_pointer()
              .tooltip(t!("app.printers.print_file"))
              .on_click(cx.async_listener(
                {
                  let name = name.clone();
                  move |this, _, _, cx| {
                    this.error = None;
                    let paths = cx.prompt_for_paths(PathPromptOptions {
                      files: true,
                      directories: false,
                      multiple: false,
                      prompt: Some(t!("app.printers.print_file").into()),
                    });
                    let (printing, name) = (cx.printing().clone(), name.clone());
                    async move {
                      let picked = paths.await.context("file picker closed")??;
                      let Some(path) = picked.and_then(|p| p.into_iter().next()) else {
                        return Ok(());
                      };
                      printing.print_file(&name, path).await
                    }
                  }
                },
                Self::show_error,
              )),
          )
          .child(
            Switch::new(format!("printer-pause-{name}"))
              .checked(printer.state != PrinterState::Stopped)
              .tooltip(t!("app.printers.enabled"))
              .on_change(cx.async_listener(
                move |this, running: &bool, _, cx| {
                  this.error = None;
                  cx.printing().set_paused(&name, !*running)
                },
                Self::show_error,
              )),
          ),
      )
      .children(printer.markers.iter().enumerate().map(|(i, marker)| {
        let id = format!("printer-marker-{}-{i}", printer.name);
        div()
          .flex()
          .gap_2()
          .items_center()
          .child(
            div()
              .w(px(96.))
              .flex_shrink_0()
              .text_xs()
              .text_color(theme.colors.muted_foreground)
              .truncate()
              .child(marker.name.clone()),
          )
          .child(
            div().flex_1().child(
              Progress::new(id)
                .value(marker.level.unwrap_or(0) as f32)
                .loading(marker.level.is_none())
                .when_some(marker.color, |p, color| p.color(rgb(color))),
            ),
          )
          .child(
            div()
              .w(px(32.))
              .text_xs()
              .text_right()
              .text_color(theme.colors.muted_foreground)
              .child(marker.level.map_or("?".into(), |l| format!("{l}%"))),
          )
      }))
  }

  /// Network printers without a queue, each with a button to add one.
  pub(super) fn network(&self, theme: &Theme, cx: &Context<'_, Self>) -> Option<impl IntoElement> {
    let found = cx.printing().new_printers(cx);
    if found.is_empty() {
      return None;
    }
    let rows = found.into_iter().map(|printer| {
      let name = printer.name.clone();
      let adding = self.adding.as_deref() == Some(&name);
      div()
        .flex()
        .w_full()
        .gap_2()
        .p_2()
        .items_center()
        .rounded_xl()
        .bg(theme.colors.background)
        .child(Icon::new(IconName::Printer).small())
        .child(
          div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .child(div().text_sm().truncate().child(printer.name.clone()))
            .child(
              div()
                .text_xs()
                .text_color(theme.colors.muted_foreground)
                .truncate()
                .child(
                  printer
                    .model
                    .clone()
                    .unwrap_or_else(|| printer.address.clone()),
                ),
            ),
        )
        .child(
          Button::new(format!("printer-add-{name}"))
            .icon(IconName::Plus)
            .small()
            .cursor_pointer()
            .with_variant(ButtonVariant::Primary)
            .tooltip(t!("app.printers.add"))
            .loading(adding)
            .disabled(self.adding.is_some())
            .on_click(cx.async_listener(
              move |this, _, _, cx| {
                this.adding = Some(name.clone());
                this.error = None;
                cx.printing().add_printer(&name, cx)
              },
              |this, result, cx| {
                this.adding = None;
                this.show_error(result, cx);
              },
            )),
        )
    });

    Some(
      div()
        .flex()
        .flex_col()
        .w_full()
        .flex_shrink_0()
        .gap_2()
        .p_2()
        .card(cx)
        .child(div().font_bold().text_sm().child(t!("app.printers.found")))
        .children(rows),
    )
  }

  pub(super) fn jobs(&self, theme: &Theme, cx: &Context<'_, Self>) -> impl IntoElement {
    let jobs = cx.printing().list_jobs(cx);
    div()
      .flex()
      .flex_col()
      .w_full()
      .when_else(jobs.len() > 2, |d| d.min_h(px(128.)), |d| d.flex_shrink_0())
      .gap_2()
      .p_2()
      .card(cx)
      .child(div().font_bold().text_sm().child(t!("app.printers.jobs")))
      .when(jobs.is_empty(), |d| {
        d.child(placeholder(theme, t!("app.printers.no_jobs")))
      })
      .child(
        div()
          .flex()
          .flex_col()
          .gap_1()
          .h_auto()
          .overflow_y_scrollbar()
          .id("printers-jobs")
          .children(jobs.iter().map(|j| self.job(theme, j, cx))),
      )
  }

  fn job(&self, theme: &Theme, job: &Job, cx: &Context<'_, Self>) -> impl IntoElement {
    let pages = match (job.pages_done, job.pages_total) {
      (Some(done), Some(total)) => Some(t!("app.printers.pages_of", done = done, total = total)),
      (Some(1), None) => Some(t!("app.printers.pages_one")),
      (Some(done), None) if done > 0 => Some(t!("app.printers.pages", done = done)),
      _ => None,
    };
    let mut details = vec![job_state(job.state).to_string(), job.printer.clone()];
    details.extend(pages.map(|p| p.to_string()));
    let progress = (job.state == JobState::Processing).then(|| {
      let bar = Progress::new(format!("job-progress-{}", job.id));
      match (job.pages_done, job.pages_total) {
        (Some(done), Some(total)) => bar.value(done as f32 * 100. / total as f32),
        _ => bar.loading(true),
      }
    });
    let id = job.id;

    div()
      .flex()
      .flex_col()
      .w_full()
      .gap_1()
      .p_2()
      .rounded_xl()
      .bg(theme.colors.background)
      .child(
        div()
          .flex()
          .gap_2()
          .items_center()
          .child(
            Icon::new(IconName::FileText)
              .small()
              .when(job.state.is_active(), |i| {
                i.text_color(theme.colors.primary)
              }),
          )
          .child(
            div()
              .flex()
              .flex_col()
              .flex_1()
              .min_w_0()
              .child(div().text_sm().truncate().child(job.name.clone()))
              .child(
                div()
                  .text_xs()
                  .text_color(theme.colors.muted_foreground)
                  .truncate()
                  .child(details.join(" · ")),
              ),
          )
          .when(job.state.is_active(), |d| {
            d.child(
              Button::new(format!("job-cancel-{id}"))
                .icon(IconName::X)
                .small()
                .with_variant(ButtonVariant::Danger)
                .tooltip(t!("app.printers.cancel_job"))
                .cursor_pointer()
                .on_click(cx.async_listener(
                  move |this, _, _, cx| {
                    this.error = None;
                    cx.printing().cancel_job(id)
                  },
                  Self::show_error,
                )),
            )
          }),
      )
      .when_some(progress, |d, p| d.child(p))
  }
}
