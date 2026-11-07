//! Local and public IP addresses

use corona_network_manager::NetworkManagerExt;
use corona_launcher::{Action, Clip, DetailView, Icon, Meta, Outcome, View, registry, simple};
use futures::AsyncReadExt;
use gpui_kit::{
  App, SharedString,
  assets::IconName,
  http_client::{AsyncBody, HttpClient},
};
use rust_i18n::t;
use std::{net::IpAddr, sync::Arc};

async fn public(client: &Arc<dyn HttpClient>, url: &str) -> Option<String> {
  let mut response = client.get(url, AsyncBody::empty(), true).await.ok()?;
  let mut body = String::new();
  // an address is 45 characters at most
  response
    .body_mut()
    .take(64)
    .read_to_string(&mut body)
    .await
    .ok()?;
  let ip: IpAddr = body.trim().parse().ok()?;
  response.status().is_success().then(|| ip.to_string())
}

fn local(cx: &App) -> Vec<(String, String)> {
  cx.network_manager()
    .list_interfaces(cx)
    .iter()
    .filter_map(|i| Some((i.name.clone(), i.ip.as_ref()?.address.to_string())))
    .collect()
}

pub(crate) fn register(cx: &mut App) {
  registry::register(
    simple(
      "ip",
      |_| Meta {
        title: t!("app.launcher.ip.title").into(),
        icon: Icon::Tile(IconName::Network, 210.),
        keywords: vec!["ip".into(), "address".into(), "network".into()],
        ..Default::default()
      },
      |_| {
        vec![Action::spawn("show", t!("app.launcher.open"), |_, cx| {
          let client = cx.http_client();
          let local = local(cx);
          async move {
            let (v4, v6) = futures::join!(
              public(&client, "https://api.ipify.org"),
              public(&client, "https://api6.ipify.org")
            );
            let mut rows: Vec<(SharedString, SharedString)> = local
              .iter()
              .map(|(name, ip)| (name.clone().into(), ip.clone().into()))
              .collect();
            let unknown = t!("app.launcher.ip.unknown").to_string();
            rows.push((
              t!("app.launcher.ip.public").into(),
              v4.clone().unwrap_or(unknown.clone()).into(),
            ));
            if let Some(v6) = &v6 {
              rows.push((t!("app.launcher.ip.public6").into(), v6.clone().into()));
            }
            let first_local = local.first().map(|(_, ip)| ip.clone()).unwrap_or_default();
            let markdown = format!(
              "# {}\n\n{}",
              first_local,
              v4.clone()
                .map(|ip| format!("{}: {ip}", t!("app.launcher.ip.public")))
                .unwrap_or_default()
            );
            let public_ip = v4;
            let local_ip = first_local.clone();
            let mut actions = Vec::new();
            if !local_ip.is_empty() {
              actions.push(Action::sync(
                "copy",
                t!("app.launcher.ip.copy_local"),
                move |_, _, _| Ok(Outcome::Copy(Clip::Text(local_ip.clone()))),
              ));
            }
            if let Some(public_ip) = public_ip {
              actions.push(Action::sync(
                "copy_public",
                t!("app.launcher.ip.copy_public"),
                move |_, _, _| Ok(Outcome::Copy(Clip::Text(public_ip.clone()))),
              ));
            }
            Ok(Outcome::Push(View::Detail(DetailView {
              title: t!("app.launcher.ip.title").into(),
              markdown: markdown.into(),
              metadata: rows,
              actions,
            })))
          }
        })]
      },
    ),
    cx,
  );
}
