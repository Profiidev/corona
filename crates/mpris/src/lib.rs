use std::time::Duration;

use anyhow::{Context, Result};
use gpui_kit::{App, AppContext, Entity, Global};
use zbus::{Connection, names::OwnedBusName, zvariant::OwnedObjectPath};

use crate::{
  listener::{listener, subscribe},
  snapshot::player_proxy,
};

pub use crate::state::{LoopStatus, PlaybackStatus, Player};

mod listener;
mod snapshot;
mod state;

#[derive(Clone)]
pub struct Mpris {
  pub players: Entity<Vec<Player>>,
  pub active: Entity<Option<String>>,
  conn: Connection,
}

impl Global for Mpris {}

pub trait MprisExt {
  fn mpris(&self) -> &Mpris;
}

impl MprisExt for App {
  fn mpris(&self) -> &Mpris {
    self.global::<Mpris>()
  }
}

impl Mpris {
  pub fn list_players<'c>(&self, cx: &'c App) -> &'c [Player] {
    self.players.read(cx)
  }

  pub fn player<'c>(&self, name: &str, cx: &'c App) -> Option<&'c Player> {
    self.list_players(cx).iter().find(|p| p.name == name)
  }

  pub fn active_player<'c>(&self, cx: &'c App) -> Option<&'c Player> {
    self.player(self.active.read(cx).as_deref()?, cx)
  }

  pub fn set_active(&self, name: &str, cx: &mut App) {
    if self.player(name, cx).is_some() {
      self.active.write(cx, Some(name.to_string()));
    }
  }

  pub fn play(&self, name: &str) -> impl Future<Output = Result<()>> + use<> {
    let (conn, name) = (self.conn.clone(), bus_name(name));
    async move { Ok(player_proxy(&conn, name?).await?.play().await?) }
  }

  pub fn pause(&self, name: &str) -> impl Future<Output = Result<()>> + use<> {
    let (conn, name) = (self.conn.clone(), bus_name(name));
    async move { Ok(player_proxy(&conn, name?).await?.pause().await?) }
  }

  pub fn play_pause(&self, name: &str) -> impl Future<Output = Result<()>> + use<> {
    let (conn, name) = (self.conn.clone(), bus_name(name));
    async move { Ok(player_proxy(&conn, name?).await?.play_pause().await?) }
  }

  pub fn next(&self, name: &str) -> impl Future<Output = Result<()>> + use<> {
    let (conn, name) = (self.conn.clone(), bus_name(name));
    async move { Ok(player_proxy(&conn, name?).await?.next().await?) }
  }

  pub fn previous(&self, name: &str) -> impl Future<Output = Result<()>> + use<> {
    let (conn, name) = (self.conn.clone(), bus_name(name));
    async move { Ok(player_proxy(&conn, name?).await?.previous().await?) }
  }

  pub fn set_position(
    &self,
    name: &str,
    position: Duration,
    cx: &App,
  ) -> impl Future<Output = Result<()>> + use<> {
    let conn = self.conn.clone();
    let name = bus_name(name);
    // the player ignores the call when the track changed in between
    let track: Result<OwnedObjectPath> = name
      .as_ref()
      .map_err(|e| anyhow::anyhow!("{e}"))
      .and_then(|n| {
        self
          .player(n, cx)
          .and_then(|p| p.track_id.clone())
          .context("the player has no track to seek in")
      });
    async move {
      let player = player_proxy(&conn, name?).await?;
      let position = i64::try_from(position.as_micros())?;
      player
        .inner()
        .call_method("SetPosition", &(track?, position))
        .await?;
      Ok(())
    }
  }

  pub fn set_volume(&self, name: &str, volume: f64) -> impl Future<Output = Result<()>> + use<> {
    let (conn, name) = (self.conn.clone(), bus_name(name));
    async move {
      anyhow::ensure!(!volume.is_nan(), "the volume is not a number");
      let volume = volume.clamp(0.0, 1.0);
      Ok(player_proxy(&conn, name?).await?.set_volume(volume).await?)
    }
  }

  pub fn set_shuffle(&self, name: &str, shuffle: bool) -> impl Future<Output = Result<()>> + use<> {
    let (conn, name) = (self.conn.clone(), bus_name(name));
    async move {
      Ok(
        player_proxy(&conn, name?)
          .await?
          .set_shuffle(shuffle)
          .await?,
      )
    }
  }

  pub fn set_loop_status(
    &self,
    name: &str,
    status: LoopStatus,
  ) -> impl Future<Output = Result<()>> + use<> {
    let (conn, name) = (self.conn.clone(), bus_name(name));
    async move {
      Ok(
        player_proxy(&conn, name?)
          .await?
          .set_loop_status(status)
          .await?,
      )
    }
  }
}

fn bus_name(name: &str) -> Result<OwnedBusName> {
  Ok(OwnedBusName::try_from(name.to_string())?)
}

pub async fn init(cx: &mut App, conn: &Connection) -> Result<()> {
  let state = Mpris {
    players: cx.new(|_| Vec::new()),
    active: cx.new(|_| None),
    conn: conn.clone(),
  };

  // subscribe before the first snapshot so no change can slip in between
  let changes = subscribe(conn).await?;
  listener(cx, conn.clone(), changes, state.clone());
  cx.set_global(state);

  Ok(())
}

#[cfg(test)]
mod tests {
  use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
  };

  use corona_utils::test_bus::{TestBus, settle, wait_until};
  use futures_lite::future::block_on;
  use gpui_kit::{self as gpui, TestAppContext};
  use zbus::{
    interface,
    zvariant::{ObjectPath, OwnedValue, Value},
  };

  use super::*;

  const PATH: &str = "/org/mpris/MediaPlayer2";

  struct Root {
    identity: String,
  }

  #[interface(name = "org.mpris.MediaPlayer2")]
  impl Root {
    #[zbus(property)]
    fn identity(&self) -> String {
      self.identity.clone()
    }

    #[zbus(property)]
    fn desktop_entry(&self) -> String {
      self.identity.to_lowercase()
    }
  }

  type Calls = Arc<Mutex<Vec<String>>>;

  struct MockPlayer {
    calls: Calls,
    status: String,
    title: String,
    volume: f64,
    shuffle: bool,
    loop_status: String,
    fail_seek: bool,
    fail_volume: bool,
  }

  impl MockPlayer {
    fn record(&self, call: String) {
      self.calls.lock().unwrap().push(call);
    }
  }

  #[interface(name = "org.mpris.MediaPlayer2.Player")]
  impl MockPlayer {
    fn play(&self) {
      self.record("Play".into());
    }
    fn pause(&self) {
      self.record("Pause".into());
    }
    fn play_pause(&self) {
      self.record("PlayPause".into());
    }
    fn next(&self) {
      self.record("Next".into());
    }
    fn previous(&self) {
      self.record("Previous".into());
    }
    fn set_position(&self, track: ObjectPath<'_>, position: i64) -> zbus::fdo::Result<()> {
      if self.fail_seek {
        return Err(zbus::fdo::Error::Failed("Cannot seek".into()));
      }
      self.record(format!("SetPosition {track} {position}"));
      Ok(())
    }

    #[zbus(property)]
    fn playback_status(&self) -> String {
      self.status.clone()
    }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
      let value = |v: Value<'_>| v.try_to_owned().unwrap();
      HashMap::from([
        (
          "mpris:trackid".to_string(),
          value(ObjectPath::from_static_str_unchecked("/track/1").into()),
        ),
        ("mpris:length".to_string(), value(180_000_000i64.into())),
        (
          "mpris:artUrl".to_string(),
          value("https://example.org/a.jpg".into()),
        ),
        ("xesam:title".to_string(), value(self.title.as_str().into())),
        ("xesam:album".to_string(), value("".into())),
        ("xesam:artist".to_string(), value(vec!["A", "B"].into())),
      ])
    }
    #[zbus(property)]
    fn position(&self) -> i64 {
      42_000_000
    }
    #[zbus(property)]
    fn rate(&self) -> f64 {
      1.0
    }
    #[zbus(property)]
    fn volume(&self) -> f64 {
      self.volume
    }
    #[zbus(property)]
    fn set_volume(&mut self, volume: f64) -> zbus::fdo::Result<()> {
      if self.fail_volume {
        return Err(zbus::fdo::Error::Failed("Volume is read-only".into()));
      }
      self.volume = volume;
      self.record(format!("Volume {volume}"));
      Ok(())
    }
    #[zbus(property)]
    fn shuffle(&self) -> bool {
      self.shuffle
    }
    #[zbus(property)]
    fn set_shuffle(&mut self, shuffle: bool) {
      self.shuffle = shuffle;
      self.record(format!("Shuffle {shuffle}"));
    }
    #[zbus(property)]
    fn loop_status(&self) -> String {
      self.loop_status.clone()
    }
    #[zbus(property)]
    fn set_loop_status(&mut self, status: String) {
      self.record(format!("LoopStatus {status}"));
      self.loop_status = status;
    }
    #[zbus(property)]
    fn can_control(&self) -> bool {
      true
    }
    #[zbus(property)]
    fn can_play(&self) -> bool {
      true
    }
    #[zbus(property)]
    fn can_pause(&self) -> bool {
      true
    }
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
      false
    }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
      true
    }
    #[zbus(property)]
    fn can_seek(&self) -> bool {
      true
    }
  }

  /// a player with nothing but the one property that is required
  struct Bare;

  #[interface(name = "org.mpris.MediaPlayer2.Player")]
  impl Bare {
    #[zbus(property)]
    fn playback_status(&self) -> String {
      "Stopped".into()
    }
  }

  struct Mock {
    conn: Connection,
    calls: Calls,
  }

  fn spawn_player(bus: &TestBus, name: &str, status: &str) -> Mock {
    spawn_custom_player(bus, name, status, 0.5, false, false)
  }

  fn spawn_custom_player(
    bus: &TestBus,
    name: &str,
    status: &str,
    volume: f64,
    fail_seek: bool,
    fail_volume: bool,
  ) -> Mock {
    let calls = Calls::default();
    let player = MockPlayer {
      calls: calls.clone(),
      status: status.into(),
      title: "Song".into(),
      volume,
      shuffle: false,
      loop_status: "None".into(),
      fail_seek,
      fail_volume,
    };
    let conn = block_on(async {
      let conn = bus.conn().await;
      conn
        .object_server()
        .at(
          PATH,
          Root {
            identity: name.into(),
          },
        )
        .await
        .unwrap();
      conn.object_server().at(PATH, player).await.unwrap();
      conn
        .request_name(format!("org.mpris.MediaPlayer2.{name}"))
        .await
        .unwrap();
      conn
    });
    Mock { conn, calls }
  }

  fn start(cx: &mut TestAppContext) -> TestBus {
    cx.executor().allow_parking();
    let bus = TestBus::new();
    let conn = block_on(bus.conn());
    cx.update(|cx| {
      cx.foreground_executor()
        .clone()
        .block_on(init(cx, &conn))
        .unwrap()
    });
    bus
  }

  fn names(cx: &mut TestAppContext) -> Vec<String> {
    cx.read(|cx| {
      cx.mpris()
        .list_players(cx)
        .iter()
        .map(|p| p.name.clone())
        .collect()
    })
  }

  fn active(cx: &mut TestAppContext) -> Option<String> {
    cx.read(|cx| cx.mpris().active.read(cx).clone())
  }

  #[test]
  fn bus_names() {
    assert!(bus_name("org.mpris.MediaPlayer2.x").is_ok());
    assert!(bus_name(":1.42").is_ok());
    for bad in ["", "no dots", "org..x", "1.starts.with.digit"] {
      assert!(bus_name(bad).is_err(), "{bad}");
    }
  }

  #[gpui::test]
  fn players_come_and_go(cx: &mut TestAppContext) {
    let bus = start(cx);
    assert!(names(cx).is_empty());
    let spotify = spawn_player(&bus, "Spotify", "Paused");
    wait_until(cx, |cx| names(cx).len() == 1);
    cx.read(|cx| {
      let player = cx.mpris().active_player(cx).unwrap();
      assert_eq!(player.name, "org.mpris.MediaPlayer2.Spotify");
      assert_eq!(
        (player.identity.as_str(), player.desktop_entry.as_deref()),
        ("Spotify", Some("spotify"))
      );
      assert_eq!(player.status, PlaybackStatus::Paused);
      assert_eq!(player.title.as_deref(), Some("Song"));
      assert_eq!(player.artists, ["A", "B"]);
      // empty strings read as missing
      assert_eq!(player.album, None);
      assert_eq!(player.art_url.as_deref(), Some("https://example.org/a.jpg"));
      assert_eq!(player.length, Some(Duration::from_secs(180)));
      assert_eq!(player.position, Duration::from_secs(42));
      assert_eq!(
        player.track_id.as_ref().map(|t| t.as_str()),
        Some("/track/1")
      );
      assert_eq!(
        (player.volume, player.shuffle, player.loop_status),
        (Some(0.5), Some(false), Some(LoopStatus::None))
      );
      assert!(player.can_play && !player.can_go_next && player.can_seek);
    });

    // a playing player takes over
    let firefox = spawn_player(&bus, "firefox", "Playing");
    wait_until(cx, |cx| names(cx).len() == 2);
    assert_eq!(
      active(cx).as_deref(),
      Some("org.mpris.MediaPlayer2.firefox")
    );
    // sorted by identity: Spotify, then firefox
    assert_eq!(names(cx)[0], "org.mpris.MediaPlayer2.Spotify");

    drop(firefox.conn);
    wait_until(cx, |cx| names(cx).len() == 1);
    assert_eq!(
      active(cx).as_deref(),
      Some("org.mpris.MediaPlayer2.Spotify")
    );
    drop(spotify.conn);
    wait_until(cx, |cx| names(cx).is_empty());
    assert_eq!(active(cx), None);
  }

  #[gpui::test]
  fn property_changes_are_picked_up(cx: &mut TestAppContext) {
    let bus = start(cx);
    let mock = spawn_player(&bus, "Spotify", "Paused");
    wait_until(cx, |cx| names(cx).len() == 1);
    block_on(async {
      let iface = mock
        .conn
        .object_server()
        .interface::<_, MockPlayer>(PATH)
        .await
        .unwrap();
      iface.get_mut().await.title = "Next song".into();
      iface
        .get()
        .await
        .metadata_changed(iface.signal_emitter())
        .await
        .unwrap();
    });
    wait_until(cx, |cx| {
      cx.read(|cx| cx.mpris().active_player(cx).unwrap().title.as_deref() == Some("Next song"))
    });
  }

  #[gpui::test]
  fn bare_players_fall_back(cx: &mut TestAppContext) {
    let bus = start(cx);
    let _bare = block_on(async {
      let conn = bus.conn().await;
      conn.object_server().at(PATH, Bare).await.unwrap();
      conn
        .request_name("org.mpris.MediaPlayer2.bare")
        .await
        .unwrap();
      conn
    });
    wait_until(cx, |cx| names(cx).len() == 1);
    cx.read(|cx| {
      let player = &cx.mpris().list_players(cx)[0];
      // named by the bus name when it has no identity
      assert_eq!(player.identity, "org.mpris.MediaPlayer2.bare");
      assert_eq!(
        (player.title.clone(), player.length, player.track_id.clone()),
        (None, None, None)
      );
      assert_eq!(
        (player.position, player.rate, player.volume),
        (Duration::ZERO, 1.0, None)
      );
      assert!(!player.can_control && !player.can_play);
    });
    // seeking needs a track
    let seek = cx.read(|cx| {
      cx.mpris()
        .set_position("org.mpris.MediaPlayer2.bare", Duration::ZERO, cx)
    });
    assert_eq!(
      block_on(seek).unwrap_err().to_string(),
      "the player has no track to seek in"
    );
  }

  #[gpui::test]
  fn controls_reach_the_player(cx: &mut TestAppContext) {
    let bus = start(cx);
    let mock = spawn_player(&bus, "Spotify", "Playing");
    wait_until(cx, |cx| names(cx).len() == 1);
    let name = "org.mpris.MediaPlayer2.Spotify";
    let mpris = cx.read(|cx| cx.mpris().clone());
    block_on(mpris.play(name)).unwrap();
    block_on(mpris.pause(name)).unwrap();
    block_on(mpris.play_pause(name)).unwrap();
    block_on(mpris.next(name)).unwrap();
    block_on(mpris.previous(name)).unwrap();
    let seek = cx.read(|cx| mpris.set_position(name, Duration::from_millis(1500), cx));
    block_on(seek).unwrap();
    block_on(mpris.set_volume(name, 1.5)).unwrap();
    block_on(mpris.set_volume(name, -1.0)).unwrap();
    block_on(mpris.set_shuffle(name, true)).unwrap();
    block_on(mpris.set_loop_status(name, LoopStatus::Playlist)).unwrap();
    assert_eq!(
      *mock.calls.lock().unwrap(),
      [
        "Play",
        "Pause",
        "PlayPause",
        "Next",
        "Previous",
        "SetPosition /track/1 1500000",
        "Volume 1",
        "Volume 0",
        "Shuffle true",
        "LoopStatus Playlist",
      ]
    );

    // bad names fail before the bus, gone players on it
    assert!(block_on(mpris.play("not a name")).is_err());
    assert!(block_on(mpris.play("org.mpris.MediaPlayer2.gone")).is_err());
    let seek = cx.read(|cx| mpris.set_position("bad name", Duration::ZERO, cx));
    assert!(block_on(seek).is_err());
  }

  #[gpui::test]
  fn nan_volume_is_not_sent(cx: &mut TestAppContext) {
    let bus = start(cx);
    let mock = spawn_player(&bus, "Spotify", "Playing");
    wait_until(cx, |cx| names(cx).len() == 1);
    let mpris = cx.read(|cx| cx.mpris().clone());
    block_on(mpris.set_volume("org.mpris.MediaPlayer2.Spotify", f64::NAN)).ok();
    assert!(!mock.calls.lock().unwrap().iter().any(|c| c.contains("NaN")));
  }

  #[gpui::test]
  fn choosing_the_active_player(cx: &mut TestAppContext) {
    let bus = start(cx);
    let _a = spawn_player(&bus, "A", "Paused");
    let _b = spawn_player(&bus, "B", "Paused");
    wait_until(cx, |cx| names(cx).len() == 2);
    let chosen = "org.mpris.MediaPlayer2.B";
    cx.update(|cx| cx.mpris().clone().set_active(chosen, cx));
    assert_eq!(active(cx).as_deref(), Some(chosen));
    // unknown players are not chosen
    cx.update(|cx| {
      cx.mpris()
        .clone()
        .set_active("org.mpris.MediaPlayer2.nope", cx)
    });
    assert_eq!(active(cx).as_deref(), Some(chosen));
    // a tie keeps the choice across snapshots
    let _c = spawn_player(&bus, "C", "Stopped");
    wait_until(cx, |cx| names(cx).len() == 3);
    settle(cx);
    assert_eq!(active(cx).as_deref(), Some(chosen));
  }

  #[gpui::test]
  fn set_position_overflow_fails(cx: &mut TestAppContext) {
    let bus = start(cx);
    let _mock = spawn_player(&bus, "Spotify", "Playing");
    wait_until(cx, |cx| names(cx).len() == 1);
    let name = "org.mpris.MediaPlayer2.Spotify";
    let seek = cx.read(|cx| cx.mpris().set_position(name, Duration::MAX, cx));
    assert!(block_on(seek).is_err());
  }

  #[gpui::test]
  fn set_position_remote_rejection_propagates_error(cx: &mut TestAppContext) {
    let bus = start(cx);
    let _mock = spawn_custom_player(&bus, "Failing", "Playing", 0.5, true, false);
    wait_until(cx, |cx| names(cx).len() == 1);
    let name = "org.mpris.MediaPlayer2.Failing";
    let seek = cx.read(|cx| cx.mpris().set_position(name, Duration::from_secs(1), cx));
    let err = block_on(seek).unwrap_err();
    assert!(err.to_string().contains("Cannot seek"));
  }

  #[gpui::test]
  fn infinite_and_readonly_volume(cx: &mut TestAppContext) {
    let bus = start(cx);
    let mock = spawn_custom_player(&bus, "Spotify", "Playing", 0.5, false, false);
    wait_until(cx, |cx| names(cx).len() == 1);
    let name = "org.mpris.MediaPlayer2.Spotify";
    let mpris = cx.read(|cx| cx.mpris().clone());
    // Positive infinity clamps to 1.0
    block_on(mpris.set_volume(name, f64::INFINITY)).unwrap();
    // Negative infinity clamps to 0.0
    block_on(mpris.set_volume(name, f64::NEG_INFINITY)).unwrap();
    assert_eq!(*mock.calls.lock().unwrap(), ["Volume 1", "Volume 0"]);

    // Read-only volume error propagation
    let _readonly = spawn_custom_player(&bus, "ReadOnly", "Playing", 0.5, false, true);
    wait_until(cx, |cx| names(cx).len() == 2);
    let ro_name = "org.mpris.MediaPlayer2.ReadOnly";
    let res = block_on(mpris.set_volume(ro_name, 0.8));
    assert!(res.unwrap_err().to_string().contains("Volume is read-only"));
  }

  #[gpui::test]
  fn unvalidated_dbus_volume_stored_as_is(cx: &mut TestAppContext) {
    let bus = start(cx);
    // Player reporting out-of-range volume -0.5
    let _p1 = spawn_custom_player(&bus, "NegativeVol", "Playing", -0.5, false, false);
    wait_until(cx, |cx| names(cx).len() == 1);
    cx.read(|cx| {
      let p = cx.mpris().active_player(cx).unwrap();
      assert_eq!(p.volume, Some(-0.5));
    });
  }

  #[gpui::test]
  fn active_observer_before_players_update(cx: &mut TestAppContext) {
    let bus = start(cx);
    let observed_active = Arc::new(Mutex::new(Vec::new()));
    let obs = observed_active.clone();
    cx.update(|cx| {
      let active = cx.mpris().active.clone();
      let mpris = cx.mpris().clone();
      cx.observe(&active, move |_active, cx| {
        obs
          .lock()
          .unwrap()
          .push(mpris.active_player(cx).map(|p| p.name.clone()));
      })
      .detach();
    });
    let _player = spawn_player(&bus, "Spotify", "Playing");
    wait_until(cx, |cx| names(cx).len() == 1);
    settle(cx);
    let list = observed_active.lock().unwrap().clone();
    // Because state.active is written before state.players in listener,
    // the observer runs before players is updated, seeing None:
    assert_eq!(list, [None]);
    // After settle, active_player is resolved properly:
    cx.read(|cx| {
      assert_eq!(
        cx.mpris().active_player(cx).map(|p| p.name.as_str()),
        Some("org.mpris.MediaPlayer2.Spotify")
      );
    });
  }
}
