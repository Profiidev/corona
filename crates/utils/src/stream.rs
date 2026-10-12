use futures_lite::{Stream, StreamExt};
use gpui_kit::App;

/// Reads `stream` on its own task into an unbounded queue, so it never backs
/// up while its consumer is busy. A zbus `MessageStream` that fills up stalls
/// the connection's reader, and then every method call on it times out.
pub fn pump<T: 'static>(
  cx: &App,
  mut stream: impl Stream<Item = T> + Unpin + 'static,
) -> flume::r#async::RecvStream<'static, T> {
  let (tx, rx) = flume::unbounded();
  cx.spawn(async move |_| {
    while let Some(item) = stream.next().await {
      if tx.send(item).is_err() {
        break;
      }
    }
  })
  .detach();
  rx.into_stream()
}

#[cfg(all(test, feature = "test-support"))]
mod tests {
  use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
  };

  use gpui_kit::TestAppContext;
  use zbus::{MatchRule, MessageStream, message::Type};

  use super::*;
  use crate::test_bus::{TestBus, wait_until};

  /// a signal burst bigger than zbus's queue must not block method calls
  #[gpui_kit::test]
  fn unread_burst_keeps_calls_working(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let bus = TestBus::new();
    let (conn, sender) = futures_lite::future::block_on(async {
      let conn = zbus::connection::Builder::address(bus.address())
        .unwrap()
        .method_timeout(Duration::from_secs(1))
        .build()
        .await
        .unwrap();
      (conn, bus.conn().await)
    });
    let rule = MatchRule::builder()
      .msg_type(Type::Signal)
      .interface("io.corona.Test")
      .unwrap()
      .build();
    let stream =
      futures_lite::future::block_on(MessageStream::for_match_rule(rule, &conn, None)).unwrap();
    let _changes = cx.update(|cx| pump(cx, stream));

    futures_lite::future::block_on(async {
      for _ in 0..200 {
        sender
          .emit_signal(None::<()>, "/", "io.corona.Test", "Ping", &())
          .await
          .unwrap();
      }
    });
    // let the burst reach the reader before calling
    let start = Instant::now();
    wait_until(cx, |_| start.elapsed() > Duration::from_millis(200));

    let done = Rc::new(Cell::new(None));
    cx.update(|cx| {
      let done = done.clone();
      cx.spawn(async move |_| {
        let dbus = zbus::fdo::DBusProxy::new(&conn).await.unwrap();
        done.set(Some(dbus.get_id().await.is_ok()));
      })
      .detach();
    });
    wait_until(cx, |_| done.get().is_some());
    assert_eq!(done.get(), Some(true));
  }
}
