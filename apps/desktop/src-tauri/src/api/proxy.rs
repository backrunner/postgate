use super::{NetworkAddress, PostGateApi, ProxyStatusView};
use crate::error::Result;
use crate::proxy::{ProxyConfig, ProxyServer, ProxyStatus};
use std::net::{IpAddr, UdpSocket};
use std::sync::Arc;

impl PostGateApi {
    pub async fn proxy_status(&self) -> Result<ProxyStatusView> {
        let proxy_guard = self.state.proxy.read();

        if let Some(ref proxy) = *proxy_guard {
            Ok(ProxyStatusView {
                status: proxy.status(),
                port: proxy.config().port,
                error: None,
            })
        } else {
            Ok(ProxyStatusView {
                status: ProxyStatus::Stopped,
                port: 0,
                error: None,
            })
        }
    }

    pub async fn start_proxy(&self, config: ProxyConfig) -> Result<ProxyStatusView> {
        if self.state.rule_engine.get_all_groups().is_empty() {
            if let Ok(db) = self.state.get_database().await {
                match db.get_rule_groups().await {
                    Ok(groups) => {
                        for group in &groups {
                            self.state.rule_engine.upsert_group(group.clone());
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to pre-load rules from database: {}", e);
                    }
                }
            }
        }

        {
            let proxy_guard = self.state.proxy.read();
            if let Some(ref proxy) = *proxy_guard {
                if proxy.status() == ProxyStatus::Running {
                    return Ok(ProxyStatusView {
                        status: ProxyStatus::Running,
                        port: proxy.config().port,
                        error: None,
                    });
                }
            }
        }

        let ca = self.state.get_or_init_ca()?;
        let mut proxy = ProxyServer::new(
            config.clone(),
            ca,
            self.state.rule_engine.clone(),
            self.state.body_storage.clone(),
            Arc::clone(&self.state),
        );
        proxy.start().await?;
        *self.state.proxy.write() = Some(proxy);

        Ok(ProxyStatusView {
            status: ProxyStatus::Running,
            port: config.port,
            error: None,
        })
    }

    pub async fn stop_proxy(&self) -> Result<ProxyStatusView> {
        let proxy = {
            let mut proxy_guard = self.state.proxy.write();
            proxy_guard.take()
        };

        if let Some(mut proxy) = proxy {
            proxy.stop().await?;
        }

        self.state.body_storage.clear().await;

        Ok(ProxyStatusView {
            status: ProxyStatus::Stopped,
            port: 0,
            error: None,
        })
    }

    pub fn set_persistence_enabled(&self, enabled: bool) {
        self.state.set_persistence_enabled(enabled);
    }

    pub fn get_persistence_enabled(&self) -> bool {
        self.state.is_persistence_enabled()
    }

    pub fn get_local_ips(&self) -> Vec<NetworkAddress> {
        let mut addresses: Vec<NetworkAddress> = Vec::new();
        addresses.push(NetworkAddress {
            ip: "127.0.0.1".to_string(),
            name: "Localhost".to_string(),
            is_default: false,
        });

        let default_ip = UdpSocket::bind("0.0.0.0:0")
            .and_then(|s| {
                s.connect("8.8.8.8:80")?;
                s.local_addr()
            })
            .ok()
            .map(|a| a.ip());

        if let Ok(ifaces) = if_addrs::get_if_addrs() {
            for iface in ifaces {
                let ip = iface.ip();
                if ip.is_loopback() || matches!(ip, IpAddr::V6(_)) {
                    continue;
                }
                addresses.push(NetworkAddress {
                    ip: ip.to_string(),
                    name: iface.name.clone(),
                    is_default: default_ip.as_ref() == Some(&ip),
                });
            }
        }

        addresses
    }
}
