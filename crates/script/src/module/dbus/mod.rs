//! `corona/dbus`: calls, properties and signals on the names a plugin's
//! manifest grants, on corona's own bus connections.

use std::{
  collections::HashMap,
  mem,
  sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use anyhow::{Context as _, Result, bail, ensure};
use corona_macros::named;
use futures::{StreamExt as _, channel::oneshot, future::BoxFuture};
use futures_lite::{FutureExt as _, future};
use gpui_kit::{Global, Subscription};
use gpui_shell::HostModule;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use serde_json::Value as Json;
use ts_rs::TS;
use zbus::{
  Connection, MatchRule, Message, MessageStream, message::Type, names::WellKnownName,
  zvariant::Structure,
};
use zbus_xml::{ArgDirection, Node};

use crate::{
  host_fn::{Glob, Module},
  module::Subscribe,
};

mod convert;

/// Corona's connections, which plugin calls go out on.
pub struct Buses {
  pub session: Connection,
  pub system: Connection,
}

impl Global for Buses {}

const DBUS: &str = "org.freedesktop.DBus";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
/// Names that would let a plugin run code or manage units, never granted.
const DENIED: [&str; 3] = [DBUS, "org.freedesktop.systemd1", "org.freedesktop.Flatpak"];
/// Subscriptions a load may make, so a plugin cannot fill the bus with match rules.
const MAX_SUBSCRIPTIONS: u32 = 32;

/// The bus names a plugin may talk to.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DbusGrant {
  /// Session bus names, e.g. `org.kde.kdeconnect`, or every name below a
  /// prefix with `org.kde.*`.
  #[serde(default, deserialize_with = "names")]
  pub session: Vec<String>,
  /// System bus names, as for `session`.
  #[serde(default, deserialize_with = "names")]
  pub system: Vec<String>,
}

/// Whether `pattern`, a name or `prefix.*`, covers the name `name`.
pub fn covers(pattern: &str, name: &str) -> bool {
  match pattern.strip_suffix(".*") {
    Some(prefix) => name
      .strip_prefix(prefix)
      .is_some_and(|rest| rest.starts_with('.')),
    None => pattern == name,
  }
}

fn names<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
  let names = Vec::<String>::deserialize(deserializer)?;
  for pattern in &names {
    let base = pattern.strip_suffix(".*").unwrap_or(pattern);
    if WellKnownName::try_from(base).is_err() {
      return Err(D::Error::custom(format!(
        "invalid bus name `{pattern}`: a well-known name, or one ending in `.*`"
      )));
    }
    if let Some(denied) = DENIED.iter().find(|denied| covers(pattern, denied)) {
      return Err(D::Error::custom(format!(
        "`{pattern}` would grant `{denied}`, which plugins cannot use"
      )));
    }
  }
  Ok(names)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Serialize, TS)]
#[serde(rename_all = "lowercase")]
enum Bus {
  Session,
  System,
}

/// A method call.
#[derive(Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
struct Call {
  bus: Bus,
  dest: String,
  path: String,
  iface: String,
  method: String,
  /// A `v` argument's type follows the JSON, `{ $type: "u", value: 3 }` sets
  /// it, also inside `av` and `a{sv}`.
  args: Option<Vec<Json>>,
  /// The types of `args`, e.g. `"sa{sv}"`. Read from the service's
  /// introspection data when left out, picking the overload that takes as
  /// many arguments.
  signature: Option<String>,
}

/// An interface on an object.
#[derive(Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
struct Object {
  bus: Bus,
  dest: String,
  path: String,
  iface: String,
}

/// A property of an interface.
#[derive(Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
struct Property {
  bus: Bus,
  dest: String,
  path: String,
  iface: String,
  name: String,
}

#[derive(Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
struct SetProperty {
  bus: Bus,
  dest: String,
  path: String,
  iface: String,
  name: String,
  /// A `v` value's type follows the JSON, `{ $type: "u", value: 3 }` sets it.
  value: Json,
  /// The property's type. Read from the introspection data when left out.
  signature: Option<String>,
}

/// The signals to receive. `sender` must be granted, or be
/// `org.freedesktop.DBus` with member `NameOwnerChanged` and a granted `arg0`.
#[derive(Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
struct Rule {
  bus: Bus,
  sender: String,
  path: Option<String>,
  path_namespace: Option<String>,
  iface: Option<String>,
  member: Option<String>,
  arg0: Option<String>,
}

#[derive(Serialize, TS)]
struct Signal {
  sender: String,
  path: String,
  iface: String,
  member: String,
  args: Vec<Json>,
}

/// Where the overloads of a method or the type of a property are cached.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
  bus: Bus,
  dest: String,
  path: String,
  iface: String,
  member: String,
  property: bool,
}

/// A subscription's stream and the name it listens to.
struct Listener {
  stream: MessageStream,
  conn: Connection,
  sender: String,
}

enum Slot {
  Idle(Box<Listener>),
  /// A `nextSignal` holds the listener; dropping this wakes it with null.
  Waiting {
    _wake: oneshot::Sender<()>,
  },
}

#[derive(Default)]
struct State {
  slots: HashMap<u32, Slot>,
  /// Subscriptions made by this load, the next id.
  made: u32,
  /// The script is gone, new subscriptions are dropped.
  closed: bool,
  signatures: HashMap<Key, Vec<Vec<String>>>,
}

#[derive(Clone)]
struct Dbus {
  grant: Arc<DbusGrant>,
  state: Arc<Mutex<State>>,
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
  state.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Bus {
  fn name(self) -> &'static str {
    match self {
      Self::Session => "session",
      Self::System => "system",
    }
  }
}

impl Dbus {
  fn new(grant: DbusGrant) -> Self {
    Self {
      grant: Arc::new(grant),
      state: Arc::default(),
    }
  }

  /// The connection to talk to `name` on, if the grant covers it.
  fn conn(&self, buses: &Buses, bus: Bus, name: &str) -> Result<Connection> {
    let (granted, conn) = match bus {
      Bus::Session => (&self.grant.session, &buses.session),
      Bus::System => (&self.grant.system, &buses.system),
    };
    ensure!(
      granted.iter().any(|pattern| covers(pattern, name)),
      "`{name}` is not in this plugin's {} bus grant",
      bus.name()
    );
    Ok(conn.clone())
  }

  fn call(&self, buses: &Buses, call: Call) -> Result<BoxFuture<'static, Result<Json>>> {
    let conn = self.conn(buses, call.bus, &call.dest)?;
    let state = self.state.clone();
    Ok(
      async move {
        let args = call.args.unwrap_or_default();
        let signature = match call.signature {
          Some(signature) => signature,
          None if args.is_empty() => String::new(),
          None => {
            let key = Key {
              bus: call.bus,
              dest: call.dest.clone(),
              path: call.path.clone(),
              iface: call.iface.clone(),
              member: call.method.clone(),
              property: false,
            };
            signature(&conn, &state, key, Some(args.len())).await?
          }
        };
        let body = convert::body(&args, &signature)?;
        let reply = send(
          &conn,
          &call.dest,
          &call.path,
          &call.iface,
          &call.method,
          body.as_ref(),
        )
        .await?;
        let mut values = convert::body_to_json(&reply)?;
        Ok(match values.len() {
          0 => Json::Null,
          1 => values.remove(0),
          _ => Json::Array(values),
        })
      }
      .boxed(),
    )
  }

  fn get_property(&self, buses: &Buses, p: Property) -> Result<BoxFuture<'static, Result<Json>>> {
    let conn = self.conn(buses, p.bus, &p.dest)?;
    Ok(
      async move {
        let reply = conn
          .call_method(
            Some(p.dest.as_str()),
            p.path.as_str(),
            Some(PROPERTIES),
            "Get",
            &(&p.iface, &p.name),
          )
          .await?;
        first(reply)
      }
      .boxed(),
    )
  }

  fn get_all(&self, buses: &Buses, o: Object) -> Result<BoxFuture<'static, Result<Json>>> {
    let conn = self.conn(buses, o.bus, &o.dest)?;
    Ok(
      async move {
        let reply = conn
          .call_method(
            Some(o.dest.as_str()),
            o.path.as_str(),
            Some(PROPERTIES),
            "GetAll",
            &(&o.iface,),
          )
          .await?;
        first(reply)
      }
      .boxed(),
    )
  }

  fn set_property(&self, buses: &Buses, p: SetProperty) -> Result<BoxFuture<'static, Result<()>>> {
    let conn = self.conn(buses, p.bus, &p.dest)?;
    let state = self.state.clone();
    Ok(
      async move {
        let signature = match p.signature {
          Some(signature) => signature,
          None => {
            let key = Key {
              bus: p.bus,
              dest: p.dest.clone(),
              path: p.path.clone(),
              iface: p.iface.clone(),
              member: p.name.clone(),
              property: true,
            };
            signature(&conn, &state, key, None).await?
          }
        };
        let signature = convert::parse(&signature)?;
        let value = convert::from_json(&p.value, &signature)?;
        conn
          .call_method(
            Some(p.dest.as_str()),
            p.path.as_str(),
            Some(PROPERTIES),
            "Set",
            &(&p.iface, &p.name, value),
          )
          .await?;
        Ok(())
      }
      .boxed(),
    )
  }

  fn subscribe(&self, buses: &Buses, rule: Rule) -> Result<BoxFuture<'static, Result<u32>>> {
    let conn = if rule.sender == DBUS && rule.member.as_deref() == Some("NameOwnerChanged") {
      let arg0 = rule
        .arg0
        .as_deref()
        .context("NameOwnerChanged needs the granted name as `arg0`")?;
      WellKnownName::try_from(arg0).with_context(|| format!("invalid bus name `{arg0}`"))?;
      self.conn(buses, rule.bus, arg0)?
    } else {
      self.conn(buses, rule.bus, &rule.sender)?
    };

    let mut builder = MatchRule::builder()
      .msg_type(Type::Signal)
      .sender(rule.sender.as_str())?;
    if let Some(path) = &rule.path {
      builder = builder.path(path.as_str())?;
    }
    if let Some(path) = &rule.path_namespace {
      builder = builder.path_namespace(path.as_str())?;
    }
    if let Some(iface) = &rule.iface {
      builder = builder.interface(iface.as_str())?;
    }
    if let Some(member) = &rule.member {
      builder = builder.member(member.as_str())?;
    }
    if let Some(arg0) = &rule.arg0 {
      builder = builder.arg(0, arg0.as_str())?;
    }
    let match_rule = builder.build().into_owned();

    let id = {
      let mut state = lock(&self.state);
      ensure!(
        state.made < MAX_SUBSCRIPTIONS,
        "at most {MAX_SUBSCRIPTIONS} subscriptions per load"
      );
      state.made += 1;
      state.made - 1
    };
    let state = self.state.clone();
    Ok(
      async move {
        let stream = MessageStream::for_match_rule(match_rule, &conn, Some(64)).await?;
        let mut state = lock(&state);
        ensure!(!state.closed, "the plugin was unloaded");
        let listener = Listener {
          stream,
          conn,
          sender: rule.sender,
        };
        state.slots.insert(id, Slot::Idle(Box::new(listener)));
        Ok(id)
      }
      .boxed(),
    )
  }

  fn next_signal(&self, id: u32) -> Result<BoxFuture<'static, Result<Option<Signal>>>> {
    let mut state = lock(&self.state);
    let Some(slot) = state.slots.get_mut(&id) else {
      return Ok(future::ready(Ok(None)).boxed());
    };
    ensure!(
      matches!(slot, Slot::Idle(_)),
      "already waiting for subscription {id}"
    );
    let (wake, woken) = oneshot::channel();
    let Slot::Idle(mut listener) = mem::replace(slot, Slot::Waiting { _wake: wake }) else {
      unreachable!("checked above")
    };
    let state = self.state.clone();
    Ok(
      async move {
        // unsubscribing drops `wake`
        let signal = listener
          .next()
          .or(async {
            woken.await.ok();
            Ok(None)
          })
          .await;
        if let Some(slot @ Slot::Waiting { .. }) = lock(&state).slots.get_mut(&id) {
          *slot = Slot::Idle(listener);
        }
        signal
      }
      .boxed(),
    )
  }

  fn unsubscribe(&self, id: u32) {
    lock(&self.state).slots.remove(&id);
  }

  fn name_has_owner(
    &self,
    buses: &Buses,
    bus: Bus,
    name: String,
  ) -> Result<BoxFuture<'static, Result<bool>>> {
    let conn = self.conn(buses, bus, &name)?;
    Ok(
      async move {
        let reply = conn
          .call_method(
            Some(DBUS),
            "/org/freedesktop/DBus",
            Some(DBUS),
            "NameHasOwner",
            &(name,),
          )
          .await?;
        Ok(reply.body().deserialize()?)
      }
      .boxed(),
    )
  }
}

impl Listener {
  async fn next(&mut self) -> Result<Option<Signal>> {
    while let Some(message) = self.stream.next().await {
      let message = message?;
      // zbus cannot match a well-known sender itself, so every signal on the
      // connection that fits the rest of the rule arrives here
      if self.sent_by_sender(&message).await {
        return signal(&message).map(Some);
      }
    }
    Ok(None)
  }

  // ponytail: a lookup per signal, cache the owner from NameOwnerChanged if it is too slow
  async fn sent_by_sender(&self, message: &Message) -> bool {
    let header = message.header();
    let Some(sender) = header.sender() else {
      return false;
    };
    let owner = self
      .conn
      .call_method(
        Some(DBUS),
        "/org/freedesktop/DBus",
        Some(DBUS),
        "GetNameOwner",
        &(&self.sender,),
      )
      .await;
    owner
      .ok()
      .and_then(|reply| reply.body().deserialize::<String>().ok())
      .is_some_and(|owner| owner == sender.as_str())
  }
}

fn signal(message: &Message) -> Result<Signal> {
  let header = message.header();
  Ok(Signal {
    sender: header.sender().map(|s| s.to_string()).unwrap_or_default(),
    path: header.path().map(|p| p.to_string()).unwrap_or_default(),
    iface: header
      .interface()
      .map(|i| i.to_string())
      .unwrap_or_default(),
    member: header.member().map(|m| m.to_string()).unwrap_or_default(),
    args: convert::body_to_json(message)?,
  })
}

/// The one value of a reply.
fn first(reply: Message) -> Result<Json> {
  convert::body_to_json(&reply)?
    .into_iter()
    .next()
    .context("empty reply")
}

async fn send(
  conn: &Connection,
  dest: &str,
  path: &str,
  iface: &str,
  method: &str,
  body: Option<&Structure<'static>>,
) -> Result<Message> {
  Ok(match body {
    Some(body) => {
      conn
        .call_method(Some(dest), path, Some(iface), method, body)
        .await?
    }
    None => {
      conn
        .call_method(Some(dest), path, Some(iface), method, &())
        .await?
    }
  })
}

/// The argument types of a method that takes `arity` arguments, or the type
/// of a property, from the service's introspection data.
async fn signature(
  conn: &Connection,
  state: &Mutex<State>,
  key: Key,
  arity: Option<usize>,
) -> Result<String> {
  let missing = || {
    format!(
      "no `{}` on `{}` at `{}` in its introspection data, pass a signature",
      key.member, key.iface, key.path
    )
  };
  let cached = lock(state).signatures.get(&key).cloned();
  let overloads = match cached {
    Some(overloads) => overloads,
    None => {
      let reply = conn
        .call_method(
          Some(key.dest.as_str()),
          key.path.as_str(),
          Some("org.freedesktop.DBus.Introspectable"),
          "Introspect",
          &(),
        )
        .await
        .with_context(missing)?;
      let xml: String = reply.body().deserialize()?;
      let node = Node::from_reader(xml.as_bytes()).with_context(missing)?;
      let overloads = overloads(&node, &key.iface, &key.member, key.property);
      ensure!(!overloads.is_empty(), missing());
      lock(state)
        .signatures
        .insert(key.clone(), overloads.clone());
      overloads
    }
  };
  pick(&overloads, &key.member, arity)
}

/// The argument types of every method `member` of `iface`, or the type of the
/// property `member` alone.
fn overloads(node: &Node, iface: &str, member: &str, property: bool) -> Vec<Vec<String>> {
  let Some(iface) = node
    .interfaces()
    .iter()
    .find(|i| i.name().as_str() == iface)
  else {
    return Vec::new();
  };
  if property {
    return iface
      .properties()
      .iter()
      .filter(|p| p.name().as_str() == member)
      .map(|p| vec![p.ty().to_string()])
      .collect();
  }
  iface
    .methods()
    .iter()
    .filter(|m| m.name().as_str() == member)
    .map(|m| {
      m.args()
        .iter()
        .filter(|arg| arg.direction() != Some(ArgDirection::Out))
        .map(|arg| arg.ty().to_string())
        .collect()
    })
    .collect()
}

/// The overload that takes `arity` arguments, any for a property. A lone
/// overload is taken whatever its arity, so the call reports the mismatch.
fn pick(overloads: &[Vec<String>], member: &str, arity: Option<usize>) -> Result<String> {
  let fits: Vec<_> = overloads
    .iter()
    .filter(|args| arity.is_none_or(|n| args.len() == n))
    .collect();
  match (fits.as_slice(), overloads) {
    (&[args], _) | ([], [args]) => Ok(args.concat()),
    _ => bail!("`{member}` is overloaded, pass a `signature`"),
  }
}

pub fn module(grant: DbusGrant, subs: &mut Vec<Subscribe>) -> HostModule {
  let dbus = Dbus::new(grant);
  let state = dbus.state.clone();
  // dropping the script wakes its waiters with null and drops its match rules
  subs.push(Subscribe::Cleanup(Subscription::new(move || {
    let mut state = lock(&state);
    state.closed = true;
    state.slots.clear();
  })));

  let d = dbus.clone();
  let module = Module::new("corona/dbus").func(named!(
    "call",
    /// Calls a method. Resolves to null without return values, the value with
    /// one, an array with more.
    move |buses: Glob<Buses>, call: Call| d.call(&buses, call)
  ));
  let d = dbus.clone();
  let module = module.func(named!(
    "getProperty",
    move |buses: Glob<Buses>, property: Property| d.get_property(&buses, property)
  ));
  let d = dbus.clone();
  let module = module.func(named!(
    "getAll",
    /// Every property of the interface, by name.
    move |buses: Glob<Buses>, object: Object| d.get_all(&buses, object)
  ));
  let d = dbus.clone();
  let module = module.func(named!(
    "setProperty",
    move |buses: Glob<Buses>, property: SetProperty| d.set_property(&buses, property)
  ));
  let d = dbus.clone();
  let module = module.func(named!(
    "subscribe",
    /// Starts receiving the signals `rule` matches, resolves to the id for
    /// `nextSignal`.
    move |buses: Glob<Buses>, rule: Rule| d.subscribe(&buses, rule)
  ));
  let d = dbus.clone();
  let module = module.func(named!(
    "nextSignal",
    /// The next signal of a subscription; null once it is unsubscribed.
    move |id: u32| d.next_signal(id)
  ));
  let d = dbus.clone();
  let module = module.func(named!("unsubscribe", move |id: u32| d.unsubscribe(id)));
  let d = dbus;
  module
    .func(named!(
      "nameHasOwner",
      /// Whether a granted name is on the bus.
      move |buses: Glob<Buses>, bus: Bus, name: String| d.name_has_owner(&buses, bus, name)
    ))
    .into()
}

#[cfg(test)]
mod tests {
  use std::collections::HashMap;

  use corona_utils::test_bus::TestBus;
  use futures_lite::future::{block_on, poll_once};
  use serde_json::json;
  use zbus::zvariant::OwnedValue;

  use super::*;

  #[derive(Default)]
  struct Test {
    count: u32,
  }

  #[zbus::interface(name = "io.corona.Test")]
  impl Test {
    fn echo(
      &self,
      map: HashMap<String, OwnedValue>,
      bytes: Vec<u8>,
      pair: (String, i32),
    ) -> (HashMap<String, OwnedValue>, Vec<u8>, (String, i32)) {
      (map, bytes, pair)
    }

    fn add(&self, a: i32, b: i32) -> i32 {
      a + b
    }

    #[zbus(property)]
    fn count(&self) -> u32 {
      self.count
    }

    #[zbus(property)]
    fn set_count(&mut self, count: u32) {
      self.count = count;
    }
  }

  /// A bus with `io.corona.Test` served at `/test`, and the connection that
  /// serves it.
  async fn serve(bus: &TestBus) -> Connection {
    let conn = bus.conn().await;
    conn
      .object_server()
      .at("/test", Test::default())
      .await
      .unwrap();
    conn.request_name("io.corona.Test").await.unwrap();
    conn
  }

  async fn buses(bus: &TestBus) -> Buses {
    let conn = bus.conn().await;
    Buses {
      session: conn.clone(),
      system: conn,
    }
  }

  fn dbus(session: &[&str]) -> Dbus {
    Dbus::new(DbusGrant {
      session: session.iter().map(|s| s.to_string()).collect(),
      system: Vec::new(),
    })
  }

  fn call(method: &str, args: Option<Json>, signature: Option<&str>) -> Call {
    Call {
      bus: Bus::Session,
      dest: "io.corona.Test".into(),
      path: "/test".into(),
      iface: "io.corona.Test".into(),
      method: method.into(),
      args: args.map(|a| serde_json::from_value(a).unwrap()),
      signature: signature.map(Into::into),
    }
  }

  fn rule(sender: &str) -> Rule {
    Rule {
      bus: Bus::Session,
      sender: sender.into(),
      path: None,
      path_namespace: None,
      iface: None,
      member: None,
      arg0: None,
    }
  }

  async fn emit(conn: &Connection, what: &str) {
    conn
      .emit_signal(None::<()>, "/test", "io.corona.Test", "Changed", &(what,))
      .await
      .unwrap();
  }

  #[test]
  fn patterns() {
    assert!(covers("org.kde.kdeconnect", "org.kde.kdeconnect"));
    assert!(!covers("org.kde.kdeconnect", "org.kde.kdeconnect.daemon"));
    assert!(covers("org.kde.*", "org.kde.kdeconnect"));
    assert!(covers("org.kde.*", "org.kde.a.b"));
    assert!(!covers("org.kde.*", "org.kde"));
    assert!(!covers("org.kde.*", "org.kdef.a"));
  }

  #[test]
  fn grants_are_validated() {
    let parse = |names: &str| toml::from_str::<DbusGrant>(&format!("session = {names}"));
    for ok in [
      r#"["org.kde.kdeconnect"]"#,
      r#"["org.kde.*"]"#,
      r#"["org.freedesktop.DBus.Foo.*"]"#,
    ] {
      assert!(parse(ok).is_ok(), "{ok}");
    }
    for bad in [
      r#"["*"]"#,
      r#"["org.*.a"]"#,
      r#"["org.kde*"]"#,
      r#"[":1.5"]"#,
      r#"[""]"#,
      r#"["org.freedesktop.DBus"]"#,
      r#"["org.freedesktop.systemd1"]"#,
      r#"["org.freedesktop.Flatpak"]"#,
      r#"["org.freedesktop.*"]"#,
      r#"["org.*"]"#,
    ] {
      assert!(parse(bad).is_err(), "{bad}");
    }
    assert!(toml::from_str::<DbusGrant>(r#"system = ["org.*"]"#).is_err());
    assert!(toml::from_str::<DbusGrant>(r#"user = []"#).is_err());
  }

  #[test]
  fn calls() {
    let bus = TestBus::new();
    block_on(async {
      let _server = serve(&bus).await;
      let buses = buses(&bus).await;
      let dbus = dbus(&["io.corona.*"]);

      // with a signature
      let sum = dbus
        .call(&buses, call("Add", Some(json!([2, 3])), Some("ii")))
        .unwrap()
        .await
        .unwrap();
      assert_eq!(sum, json!(5));

      // from the introspection data, which is then cached
      let args = json!([{ "a": 1, "b": "x" }, [1, 2], ["s", 4]]);
      let echo = dbus
        .call(&buses, call("Echo", Some(args.clone()), None))
        .unwrap()
        .await
        .unwrap();
      assert_eq!(echo, args);
      assert_eq!(
        lock(&dbus.state).signatures.values().collect::<Vec<_>>(),
        [&[vec!["a{sv}", "ay", "(si)"]]]
      );

      let error = dbus
        .call(&buses, call("Missing", Some(json!([1])), None))
        .unwrap()
        .await
        .unwrap_err();
      assert!(error.to_string().contains("pass a signature"), "{error}");
      let error = dbus
        .call(&buses, call("Add", Some(json!([1])), Some("ii")))
        .unwrap()
        .await
        .unwrap_err();
      assert!(error.to_string().contains("takes 2"), "{error}");
    });
  }

  #[test]
  fn overloads_are_picked_by_arity() {
    let node = Node::from_reader(
      r#"<node><interface name="a.B">
        <method name="Send"><arg type="s" direction="in"/></method>
        <method name="Send"><arg type="s"/><arg type="as"/><arg type="i" direction="out"/></method>
        <method name="Send"><arg type="o"/><arg type="u"/></method>
        <method name="Ping"><arg type="u"/></method>
        <property name="Ping" type="b" access="read"/>
      </interface></node>"#
        .as_bytes(),
    )
    .unwrap();
    let send = overloads(&node, "a.B", "Send", false);
    assert_eq!(pick(&send, "Send", Some(1)).unwrap(), "s");
    let error = pick(&send, "Send", Some(2)).unwrap_err();
    assert!(error.to_string().contains("overloaded"), "{error}");
    assert!(pick(&send, "Send", Some(3)).is_err());
    // one method is taken whatever its arity
    let ping = overloads(&node, "a.B", "Ping", false);
    assert_eq!(pick(&ping, "Ping", Some(2)).unwrap(), "u");
    let ping = overloads(&node, "a.B", "Ping", true);
    assert_eq!(pick(&ping, "Ping", None).unwrap(), "b");
    assert!(overloads(&node, "a.C", "Send", false).is_empty());
  }

  #[test]
  fn properties() {
    let bus = TestBus::new();
    block_on(async {
      let _server = serve(&bus).await;
      let buses = buses(&bus).await;
      let dbus = dbus(&["io.corona.Test"]);
      let property = |name: &str| Property {
        bus: Bus::Session,
        dest: "io.corona.Test".into(),
        path: "/test".into(),
        iface: "io.corona.Test".into(),
        name: name.into(),
      };

      let count = dbus.get_property(&buses, property("Count")).unwrap().await;
      assert_eq!(count.unwrap(), json!(0));

      // typed from the introspection data
      let set = SetProperty {
        bus: Bus::Session,
        dest: "io.corona.Test".into(),
        path: "/test".into(),
        iface: "io.corona.Test".into(),
        name: "Count".into(),
        value: json!(7),
        signature: None,
      };
      dbus.set_property(&buses, set).unwrap().await.unwrap();

      let object = Object {
        bus: Bus::Session,
        dest: "io.corona.Test".into(),
        path: "/test".into(),
        iface: "io.corona.Test".into(),
      };
      let all = dbus.get_all(&buses, object).unwrap().await.unwrap();
      assert_eq!(all, json!({ "Count": 7 }));
    });
  }

  #[test]
  fn ungranted_names_fail_before_any_io() {
    let bus = TestBus::new();
    let buses = block_on(buses(&bus));
    drop(bus);
    let dbus = dbus(&["io.corona.Other"]);
    let error = dbus.call(&buses, call("Add", None, None)).err().unwrap();
    assert!(
      error
        .to_string()
        .contains("not in this plugin's session bus grant"),
      "{error}"
    );
    // the system bus has its own grant
    let mut on_system = call("Add", None, None);
    on_system.bus = Bus::System;
    let dbus = super::Dbus::new(DbusGrant {
      session: vec!["io.corona.Test".into()],
      system: Vec::new(),
    });
    assert!(dbus.call(&buses, on_system).is_err());
    assert!(
      dbus
        .name_has_owner(&buses, Bus::Session, "io.corona.Other".into())
        .is_err()
    );
    assert!(dbus.subscribe(&buses, rule("io.corona.Other")).is_err());
  }

  #[test]
  fn signals() {
    let bus = TestBus::new();
    block_on(async {
      let server = serve(&bus).await;
      let buses = buses(&bus).await;
      let dbus = dbus(&["io.corona.Test"]);

      let id = dbus
        .subscribe(&buses, rule("io.corona.Test"))
        .unwrap()
        .await
        .unwrap();
      emit(&server, "a").await;
      let signal = dbus.next_signal(id).unwrap().await.unwrap().unwrap();
      assert_eq!(signal.path, "/test");
      assert_eq!(signal.iface, "io.corona.Test");
      assert_eq!(signal.member, "Changed");
      assert_eq!(signal.args, [json!("a")]);

      // the same signal from another connection is not the granted name's
      let other = bus.conn().await;
      emit(&other, "spoofed").await;
      emit(&server, "b").await;
      let signal = dbus.next_signal(id).unwrap().await.unwrap().unwrap();
      assert_eq!(signal.args, [json!("b")]);

      // one waiter at a time; unsubscribing wakes it with null
      let mut waiting = dbus.next_signal(id).unwrap();
      assert!(poll_once(&mut waiting).await.is_none());
      assert!(dbus.next_signal(id).is_err());
      dbus.unsubscribe(id);
      assert!(waiting.await.unwrap().is_none());
      assert!(dbus.next_signal(id).unwrap().await.unwrap().is_none());

      // so does unloading
      let id = dbus
        .subscribe(&buses, rule("io.corona.Test"))
        .unwrap()
        .await
        .unwrap();
      let mut waiting = dbus.next_signal(id).unwrap();
      assert!(poll_once(&mut waiting).await.is_none());
      lock(&dbus.state).slots.clear();
      assert!(waiting.await.unwrap().is_none());
    });
  }

  #[test]
  fn subscriptions_are_capped() {
    let bus = TestBus::new();
    block_on(async {
      let buses = buses(&bus).await;
      let dbus = dbus(&["io.corona.Test"]);
      for _ in 0..MAX_SUBSCRIPTIONS {
        dbus
          .subscribe(&buses, rule("io.corona.Test"))
          .unwrap()
          .await
          .unwrap();
      }
      let error = dbus
        .subscribe(&buses, rule("io.corona.Test"))
        .err()
        .unwrap();
      assert!(error.to_string().contains("at most 32"), "{error}");
    });
  }

  #[test]
  fn name_owner_changes() {
    let bus = TestBus::new();
    block_on(async {
      let buses = buses(&bus).await;
      let dbus = dbus(&["io.corona.Test"]);
      let owner_changed = |arg0: Option<&str>| Rule {
        member: Some("NameOwnerChanged".into()),
        arg0: arg0.map(Into::into),
        ..rule(DBUS)
      };
      assert!(dbus.subscribe(&buses, rule(DBUS)).is_err());
      assert!(dbus.subscribe(&buses, owner_changed(None)).is_err());
      assert!(
        dbus
          .subscribe(&buses, owner_changed(Some("io.corona.Other")))
          .is_err()
      );
      assert!(
        dbus
          .subscribe(&buses, owner_changed(Some("io.corona.*")))
          .is_err()
      );

      let id = dbus
        .subscribe(&buses, owner_changed(Some("io.corona.Test")))
        .unwrap()
        .await
        .unwrap();
      assert!(
        !dbus
          .name_has_owner(&buses, Bus::Session, "io.corona.Test".into())
          .unwrap()
          .await
          .unwrap()
      );
      let server = serve(&bus).await;
      let signal = dbus.next_signal(id).unwrap().await.unwrap().unwrap();
      assert_eq!(signal.member, "NameOwnerChanged");
      assert_eq!(signal.args[0], json!("io.corona.Test"));
      assert_eq!(
        signal.args[2],
        json!(server.unique_name().unwrap().as_str())
      );
      assert!(
        dbus
          .name_has_owner(&buses, Bus::Session, "io.corona.Test".into())
          .unwrap()
          .await
          .unwrap()
      );
    });
  }
}
