//! `corona ipc network`

use clap::Subcommand;
use clap_complete::{ArgValueCandidates, CompletionCandidate};
use corona_ipc::{
  IpcCommandSend, Switch,
  cli::{described, done, plain, print_switch},
};

use corona_shell::ipc::network::{
  network::{
    ConnectWifi, DisconnectWifi, ForgetWifi, HiddenRequest, Hotspot, JoinHidden, ListVpns,
    ListWifi, Portal, Rescan, Security, SwitchVpn,
  },
  radio::Wifi,
};

#[derive(Subcommand)]
pub enum Commands {
  /// List the Wi-Fi networks in range
  List,
  /// Connect to a Wi-Fi network
  Connect {
    #[arg(add = ArgValueCandidates::new(ssids))]
    ssid: String,
  },
  /// Disconnect from the Wi-Fi network
  Disconnect,
  /// Forget a saved Wi-Fi network
  Forget {
    #[arg(add = ArgValueCandidates::new(ssids))]
    ssid: String,
  },
  /// Join a network that hides its name
  JoinHidden {
    ssid: String,
    #[arg(long, default_value = "wpa", ignore_case = true)]
    security: Security,
    #[arg(long)]
    password: Option<String>,
  },
  /// Scan for networks
  Rescan,
  /// List VPNs
  Vpns,
  /// A VPN by name or uuid: on, off, toggle or status
  Vpn {
    #[arg(add = ArgValueCandidates::new(vpns))]
    name: String,
    switch: Switch,
  },
  /// The hotspot: on, off, toggle or status
  Hotspot { switch: Switch },
  /// Open the captive portal's login page
  Portal,
  /// Wi-Fi radio: on, off, toggle or status
  Wifi { switch: Switch },
}

impl Commands {
  pub fn run(self) {
    match self {
      Commands::List => match ListWifi::send(()) {
        Ok(networks) => {
          for n in networks {
            let lock = if n.secured { "🔒" } else { "  " };
            println!("{:>3}% {lock} {:<10} {}", n.strength, n.status, n.ssid);
          }
        }
        Err(e) => tracing::error!("Failed to list networks: {}", e),
      },
      Commands::Connect { ssid } => done(ConnectWifi::send(ssid), "connect"),
      Commands::Disconnect => done(DisconnectWifi::send(()), "disconnect"),
      Commands::Forget { ssid } => done(ForgetWifi::send(ssid), "forget the network"),
      Commands::JoinHidden {
        ssid,
        security,
        password,
      } => done(
        JoinHidden::send(HiddenRequest {
          ssid,
          security,
          password,
        }),
        "join the network",
      ),
      Commands::Rescan => match Rescan::send(()) {
        Ok(true) => {}
        Ok(false) => println!("timed out"),
        Err(e) => tracing::error!("Failed to scan: {}", e),
      },
      Commands::Vpns => match ListVpns::send(()) {
        Ok(vpns) => {
          for v in vpns {
            println!("{} {} {}", if v.active { "*" } else { " " }, v.uuid, v.name);
          }
        }
        Err(e) => tracing::error!("Failed to list VPNs: {}", e),
      },
      Commands::Vpn { name, switch } => {
        print_switch(SwitchVpn::send((name, switch)), switch, "the VPN")
      }
      Commands::Hotspot { switch } => print_switch(Hotspot::send(switch), switch, "the hotspot"),
      Commands::Portal => done(Portal::send(()), "open the portal"),
      Commands::Wifi { switch } => print_switch(Wifi::send(switch), switch, "Wi-Fi"),
    }
  }
}

fn ssids() -> Vec<CompletionCandidate> {
  described(
    ListWifi::send(())
      .unwrap_or_default()
      .into_iter()
      .map(|n| (n.ssid, format!("{}% {}", n.strength, n.status))),
  )
}

fn vpns() -> Vec<CompletionCandidate> {
  plain(
    ListVpns::send(())
      .unwrap_or_default()
      .into_iter()
      .map(|v| v.name),
  )
}
