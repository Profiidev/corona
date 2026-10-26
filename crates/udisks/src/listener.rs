use anyhow::Result;
use corona_utils::{entity::WriteChangedExt, error::ErrorLogExt, stream::pump};
use futures_lite::{Stream, StreamExt, future::poll_once};
use gpui_kit::{App, AsyncApp};
use zbus::{
  Connection, MatchRule, Message, MessageStream, message::Type, zvariant::OwnedObjectPath,
};

use crate::{SERVICE, Udisks, snapshot::snapshot, state::JobCompleted};

const JOB: &str = "org.freedesktop.UDisks2.Job";

/// A signal of udisksd's, or its name changing owner
pub enum Change {
  Other,
  /// `Job.Completed(success, message)` of the job at the path
  Completed(OwnedObjectPath, bool, String),
}

fn change(message: zbus::Result<Message>) -> Change {
  let Ok(message) = message else {
    return Change::Other;
  };
  let header = message.header();
  let completed = header.interface().is_some_and(|i| i.as_str() == JOB)
    && header.member().is_some_and(|m| m.as_str() == "Completed");
  match (completed, header.path()) {
    (true, Some(path)) => match message.body().deserialize::<(bool, String)>() {
      Ok((success, text)) => Change::Completed(path.to_owned().into(), success, text),
      Err(_) => Change::Other,
    },
    _ => Change::Other,
  }
}

/// fires on udisksd's signals (objects added and removed, properties, jobs) and on it starting
/// or quitting
pub async fn subscribe(conn: &Connection) -> Result<impl Stream<Item = Change> + Unpin + use<>> {
  let signals = MatchRule::builder()
    .msg_type(Type::Signal)
    .sender(SERVICE)?
    .build();
  let owner = MatchRule::builder()
    .msg_type(Type::Signal)
    .sender("org.freedesktop.DBus")?
    .interface("org.freedesktop.DBus")?
    .member("NameOwnerChanged")?
    .arg(0, SERVICE)?
    .build();
  let signals = MessageStream::for_match_rule(signals, conn, None).await?;
  let owner = MessageStream::for_match_rule(owner, conn, None).await?;
  Ok(signals.map(change).or(owner.map(|_| Change::Other)))
}

/// Says which job completed, with what it was as last seen: it is gone by the next snapshot
fn completed(change: Change, state: &Udisks, cx: &mut AsyncApp) {
  let Change::Completed(job, success, message) = change else {
    return;
  };
  let known = state
    .jobs
    .read_with(cx, |jobs, _| jobs.iter().find(|j| j.path == job).cloned());
  let event = JobCompleted {
    operation: known
      .as_ref()
      .map(|j| j.operation.clone())
      .unwrap_or_default(),
    objects: known.map(|j| j.objects).unwrap_or_default(),
    job,
    success,
    message,
  };
  state.job_completed.update(cx, |last, cx| {
    *last = Some(event);
    cx.notify();
  });
}

pub fn listener(
  cx: &mut App,
  conn: Connection,
  changes: impl Stream<Item = Change> + Unpin + 'static,
  state: Udisks,
) {
  let mut changes = pump(cx, changes);
  cx.spawn(async move |cx| {
    loop {
      let snapshot = snapshot(&conn).await.log_err().unwrap_or_default();
      state.running.write_changed(cx, snapshot.running);
      state.drives.write_changed(cx, snapshot.drives);
      state.blocks.write_changed(cx, snapshot.blocks);
      state.jobs.write_changed(cx, snapshot.jobs);
      state.volumes.write_changed(cx, snapshot.volumes);

      let Some(next) = changes.next().await else {
        break;
      };
      completed(next, &state, cx);
      // drain the rest of the burst already queued, one snapshot covers it
      while let Some(Some(next)) = poll_once(changes.next()).await {
        completed(next, &state, cx);
      }
    }
  })
  .detach();
}
