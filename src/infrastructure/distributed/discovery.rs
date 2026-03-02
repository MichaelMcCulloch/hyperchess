use std::net::ToSocketAddrs;

/// Discovers worker pod IPs via K8s headless service DNS resolution.
pub struct WorkerDiscovery {
    service_dns: String,
    grpc_port: u16,
    own_pod_ip: String,
}

impl WorkerDiscovery {
    pub fn new(service_dns: String, grpc_port: u16) -> Self {
        let own_pod_ip = std::env::var("POD_IP").unwrap_or_default();
        Self {
            service_dns,
            grpc_port,
            own_pod_ip,
        }
    }

    /// Discover all worker endpoints, excluding our own pod IP.
    /// Returns list of "http://ip:port" endpoints.
    pub async fn discover_workers(&self) -> Vec<String> {
        if self.service_dns.is_empty() {
            return Vec::new();
        }

        let dns_with_port = format!("{}:{}", self.service_dns, self.grpc_port);

        // DNS resolution of headless service returns all pod IPs
        let addrs = match tokio::task::spawn_blocking({
            let dns = dns_with_port.clone();
            move || dns.to_socket_addrs()
        })
        .await
        {
            Ok(Ok(addrs)) => addrs.collect::<Vec<_>>(),
            Ok(Err(e)) => {
                eprintln!(
                    "[discovery] DNS resolution failed for {}: {}",
                    dns_with_port, e
                );
                return Vec::new();
            }
            Err(e) => {
                eprintln!("[discovery] DNS task failed: {}", e);
                return Vec::new();
            }
        };

        let mut workers = Vec::new();
        for addr in addrs {
            let ip = addr.ip().to_string();
            // Exclude our own pod
            if ip != self.own_pod_ip {
                workers.push(format!("http://{}:{}", ip, self.grpc_port));
            }
        }

        eprintln!(
            "[discovery] Found {} workers (excluding self): {:?}",
            workers.len(),
            workers
        );

        workers
    }
}
