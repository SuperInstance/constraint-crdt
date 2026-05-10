//! # PLATO HTTP Client
//!
//! Merge constraint CRDTs directly with the PLATO tile server.
//! Reads rooms, writes tiles, merges constraint state across fleet.

use crate::tile::FleetTile;
use crate::state::ConstraintState;
use crate::delta::ConstraintDelta;

/// PLATO server client for constraint CRDT operations.
pub struct PlatoClient {
    base_url: String,
    client: reqwest::blocking::Client,
}

/// Room data from PLATO
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PlatoRoom {
    pub id: String,
    pub tiles: Option<Vec<serde_json::Value>>,
    pub tile_count: Option<usize>,
}

impl PlatoClient {
    /// Create a client pointing at the PLATO server.
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new()),
        }
    }

    /// Default client pointing at the fleet PLATO server.
    pub fn fleet() -> Self {
        Self::new("http://147.224.38.131:8847")
    }

    /// List all rooms, optionally filtered by prefix.
    pub fn list_rooms(&self, prefix: Option<&str>) -> Result<Vec<String>, String> {
        let url = match prefix {
            Some(p) => format!("{}/rooms?prefix={}", self.base_url, p),
            None => format!("{}/rooms", self.base_url),
        };
        let resp = self.client.get(&url)
            .send()
            .map_err(|e| format!("HTTP error: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("PLATO returned {}", resp.status()));
        }

        resp.json::<Vec<String>>()
            .map_err(|e| format!("Parse error: {}", e))
    }

    /// Get room data.
    pub fn get_room(&self, room_id: &str) -> Result<PlatoRoom, String> {
        let url = format!("{}/room/{}", self.base_url, room_id);
        let resp = self.client.get(&url)
            .send()
            .map_err(|e| format!("HTTP error: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("PLATO returned {}", resp.status()));
        }

        resp.json::<PlatoRoom>()
            .map_err(|e| format!("Parse error: {}", e))
    }

    /// Submit a tile to a room.
    pub fn submit_tile(&self, room_id: &str, tile: &FleetTile) -> Result<(), String> {
        let url = format!("{}/room/{}/tile", self.base_url, room_id);
        let body = tile.to_json();

        let resp = self.client.post(&url)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .map_err(|e| format!("HTTP error: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("PLATO returned {}", resp.status()));
        }

        Ok(())
    }

    /// Send a delta to the PLATO server for a room.
    pub fn send_delta(&self, room_id: &str, delta: &ConstraintDelta) -> Result<(), String> {
        let url = format!("{}/room/{}/delta", self.base_url, room_id);
        let body = serde_json::to_vec(delta).unwrap_or_default();

        let resp = self.client.post(&url)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .map_err(|e| format!("HTTP error: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("PLATO returned {}", resp.status()));
        }

        Ok(())
    }

    /// Health check — returns true if PLATO is reachable.
    pub fn is_healthy(&self) -> bool {
        let url = format!("{}/rooms", self.base_url);
        self.client.get(&url).send().map(|r| r.status().is_success()).unwrap_or(false)
    }

    /// Get base URL
    pub fn url(&self) -> &str {
        &self.base_url
    }
}

impl std::fmt::Display for PlatoClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PlatoClient({})", self.base_url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = PlatoClient::new("http://localhost:8847");
        assert_eq!(client.url(), "http://localhost:8847");
    }

    #[test]
    fn test_fleet_client() {
        let client = PlatoClient::fleet();
        assert!(client.url().contains("147.224.38.131"));
    }

    #[test]
    fn test_display() {
        let client = PlatoClient::new("http://example.com:8847");
        let s = format!("{}", client);
        assert!(s.contains("example.com"));
    }
}
