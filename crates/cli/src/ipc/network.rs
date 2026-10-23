use clap::Subcommand;
use corona_ipc::IpcCommandSend;
use corona_shell::commands::{
  network::{
    ConnectWifi, DisconnectWifi, ForgetWifi, HiddenRequest, Hotspot, JoinHidden, ListVpns,
    ListWifi, Portal, Rescan, Security, SwitchVpn,
  },
  radio::Switch,
};

use crate::ipc::{done, print_switch};

#[derive(Subcommand)]
pub enum NetworkCommands {
  /// List the Wi-Fi networks in range
  List,
  /// Connect to a Wi-Fi network
  Connect { ssid: String },
  /// Disconnect from the Wi-Fi network
  Disconnect,
  /// Forget a saved Wi-Fi network
  Forget { ssid: String },
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
  Vpn { name: String, switch: Switch },
  /// The hotspot: on, off, toggle or status
  Hotspot { switch: Switch },
  /// Open the captive portal's login page
  Portal,
}

impl NetworkCommands {
  pub fn execute(self) {
    match self {
      NetworkCommands::List => match ListWifi::send(()) {
        Ok(networks) => {
          for n in networks {
            let lock = if n.secured { "🔒" } else { "  " };
            println!("{:>3}% {lock} {:<10} {}", n.strength, n.status, n.ssid);
          }
        }
        Err(e) => tracing::error!("Failed to list networks: {}", e),
      },
      NetworkCommands::Connect { ssid } => done(ConnectWifi::send(ssid), "connect"),
      NetworkCommands::Disconnect => done(DisconnectWifi::send(()), "disconnect"),
      NetworkCommands::Forget { ssid } => done(ForgetWifi::send(ssid), "forget the network"),
      NetworkCommands::JoinHidden {
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
      NetworkCommands::Rescan => match Rescan::send(()) {
        Ok(true) => {}
        Ok(false) => println!("timed out"),
        Err(e) => tracing::error!("Failed to scan: {}", e),
      },
      NetworkCommands::Vpns => match ListVpns::send(()) {
        Ok(vpns) => {
          for v in vpns {
            println!("{} {} {}", if v.active { "*" } else { " " }, v.uuid, v.name);
          }
        }
        Err(e) => tracing::error!("Failed to list VPNs: {}", e),
      },
      NetworkCommands::Vpn { name, switch } => {
        print_switch(SwitchVpn::send((name, switch)), switch, "the VPN")
      }
      NetworkCommands::Hotspot { switch } => {
        print_switch(Hotspot::send(switch), switch, "the hotspot")
      }
      NetworkCommands::Portal => done(Portal::send(()), "open the portal"),
    }
  }
}
