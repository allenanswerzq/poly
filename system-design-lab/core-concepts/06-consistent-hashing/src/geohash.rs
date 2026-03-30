#![allow(dead_code, unused_variables, unused_imports)]
//! # Geohashing
//!
//! Encodes latitude/longitude into a compact string that represents a grid cell.
//! Nearby points share common prefixes → efficient spatial proximity queries.
//!
//! Used in: Uber, DoorDash, Yelp, Redis GEO commands, Elasticsearch,
//! location-based search, ride matching, delivery zone assignment.
//!
//! Precision levels:
//!   1 char  → ~5000 km
//!   3 chars → ~156 km
//!   5 chars → ~4.9 km
//!   7 chars → ~153 m
//!   9 chars → ~4.8 m
//!  12 chars → ~3.7 cm

const BASE32: &[u8; 32] = b"0123456789bcdefghjkmnpqrstuvwxyz";

// =============================================================================
// Geohash Encode/Decode
// =============================================================================

/// Encode (lat, lon) to a geohash string with given precision (1-12 chars).
pub fn encode(lat: f64, lon: f64, precision: usize) -> String {
    let mut lat_range = (-90.0_f64, 90.0_f64);
    let mut lon_range = (-180.0_f64, 180.0_f64);
    let mut hash = String::with_capacity(precision);
    let mut bits = 0u8;
    let mut bit_count = 0;
    let mut is_lon = true; // alternate: longitude first, then latitude

    let total_bits = precision * 5; // 5 bits per base32 char

    for _ in 0..total_bits {
        if is_lon {
            let mid = (lon_range.0 + lon_range.1) / 2.0;
            if lon >= mid {
                bits = (bits << 1) | 1;
                lon_range.0 = mid;
            } else {
                bits <<= 1;
                lon_range.1 = mid;
            }
        } else {
            let mid = (lat_range.0 + lat_range.1) / 2.0;
            if lat >= mid {
                bits = (bits << 1) | 1;
                lat_range.0 = mid;
            } else {
                bits <<= 1;
                lat_range.1 = mid;
            }
        }
        is_lon = !is_lon;
        bit_count += 1;

        if bit_count == 5 {
            hash.push(BASE32[bits as usize] as char);
            bits = 0;
            bit_count = 0;
        }
    }

    hash
}

/// Decode a geohash string back to (lat, lon) center point.
pub fn decode(geohash: &str) -> (f64, f64) {
    let mut lat_range = (-90.0_f64, 90.0_f64);
    let mut lon_range = (-180.0_f64, 180.0_f64);
    let mut is_lon = true;

    for ch in geohash.chars() {
        let idx = BASE32.iter().position(|&c| c == ch as u8).unwrap_or(0);
        for bit in (0..5).rev() {
            let b = (idx >> bit) & 1;
            if is_lon {
                let mid = (lon_range.0 + lon_range.1) / 2.0;
                if b == 1 {
                    lon_range.0 = mid;
                } else {
                    lon_range.1 = mid;
                }
            } else {
                let mid = (lat_range.0 + lat_range.1) / 2.0;
                if b == 1 {
                    lat_range.0 = mid;
                } else {
                    lat_range.1 = mid;
                }
            }
            is_lon = !is_lon;
        }
    }

    let lat = (lat_range.0 + lat_range.1) / 2.0;
    let lon = (lon_range.0 + lon_range.1) / 2.0;
    (lat, lon)
}

/// Get bounding box (lat_min, lat_max, lon_min, lon_max) for a geohash.
pub fn bounds(geohash: &str) -> (f64, f64, f64, f64) {
    let mut lat_range = (-90.0_f64, 90.0_f64);
    let mut lon_range = (-180.0_f64, 180.0_f64);
    let mut is_lon = true;

    for ch in geohash.chars() {
        let idx = BASE32.iter().position(|&c| c == ch as u8).unwrap_or(0);
        for bit in (0..5).rev() {
            let b = (idx >> bit) & 1;
            if is_lon {
                let mid = (lon_range.0 + lon_range.1) / 2.0;
                if b == 1 {
                    lon_range.0 = mid;
                } else {
                    lon_range.1 = mid;
                }
            } else {
                let mid = (lat_range.0 + lat_range.1) / 2.0;
                if b == 1 {
                    lat_range.0 = mid;
                } else {
                    lat_range.1 = mid;
                }
            }
            is_lon = !is_lon;
        }
    }

    (lat_range.0, lat_range.1, lon_range.0, lon_range.1)
}

// =============================================================================
// Neighbor Finding
// =============================================================================
// Key operation in proximity queries: find the 8 adjacent geohash cells.

/// Get the 8 neighboring geohash cells (N, NE, E, SE, S, SW, W, NW).
pub fn neighbors(geohash: &str) -> Vec<String> {
    let (lat, lon) = decode(geohash);
    let (lat_min, lat_max, lon_min, lon_max) = bounds(geohash);
    let lat_delta = lat_max - lat_min;
    let lon_delta = lon_max - lon_min;
    let precision = geohash.len();

    let directions = [
        (lat_delta, 0.0),        // N
        (lat_delta, lon_delta),   // NE
        (0.0, lon_delta),         // E
        (-lat_delta, lon_delta),  // SE
        (-lat_delta, 0.0),        // S
        (-lat_delta, -lon_delta), // SW
        (0.0, -lon_delta),        // W
        (lat_delta, -lon_delta),  // NW
    ];

    directions
        .iter()
        .map(|(dlat, dlon)| encode(lat + dlat, lon + dlon, precision))
        .collect()
}

// =============================================================================
// Proximity Search (simple spatial index)
// =============================================================================

pub struct GeoIndex {
    /// geohash → list of (id, lat, lon)
    cells: std::collections::HashMap<String, Vec<(String, f64, f64)>>,
    precision: usize,
}

impl GeoIndex {
    pub fn new(precision: usize) -> Self {
        Self {
            cells: std::collections::HashMap::new(),
            precision,
        }
    }

    pub fn insert(&mut self, id: &str, lat: f64, lon: f64) {
        let hash = encode(lat, lon, self.precision);
        self.cells
            .entry(hash)
            .or_default()
            .push((id.to_string(), lat, lon));
    }

    /// Find all points near (lat, lon) within the same and adjacent cells.
    pub fn nearby(&self, lat: f64, lon: f64) -> Vec<(&str, f64, f64, f64)> {
        let center_hash = encode(lat, lon, self.precision);
        let neighbor_hashes = neighbors(&center_hash);

        let mut results = Vec::new();

        // Check center cell + 8 neighbors
        for hash in std::iter::once(&center_hash).chain(neighbor_hashes.iter()) {
            if let Some(points) = self.cells.get(hash) {
                for (id, plat, plon) in points {
                    let dist = haversine_km(lat, lon, *plat, *plon);
                    results.push((id.as_str(), *plat, *plon, dist));
                }
            }
        }

        results.sort_by(|a, b| a.3.partial_cmp(&b.3).unwrap());
        results
    }
}

/// Haversine distance in km.
fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0; // Earth radius in km
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();
    r * c
}

// =============================================================================
// Demo
// =============================================================================

pub fn demo() {
    println!("=== Geohashing ===\n");

    // Famous locations
    let locations = [
        ("Statue of Liberty", 40.6892, -74.0445),
        ("Eiffel Tower",      48.8584,   2.2945),
        ("Tokyo Tower",       35.6586, 139.7454),
        ("Sydney Opera House",-33.8568, 151.2153),
        ("Golden Gate Bridge", 37.8199, -122.4783),
    ];

    println!("{:<25} {:>10} {:>10}  {:<12} {:<7}", "Location", "Lat", "Lon", "Geohash(7)", "Hash(5)");
    println!("{}", "-".repeat(78));

    for (name, lat, lon) in &locations {
        let gh7 = encode(*lat, *lon, 7);
        let gh5 = encode(*lat, *lon, 5);
        println!("{:<25} {:>10.4} {:>10.4}  {:<12} {:<7}", name, lat, lon, gh7, gh5);
    }

    // Decode round-trip
    println!("\n--- Encode/Decode Round-trip ---");
    let (lat, lon) = (40.6892, -74.0445);
    let hash = encode(lat, lon, 9);
    let (dlat, dlon) = decode(&hash);
    println!("  Original:  ({lat}, {lon})");
    println!("  Geohash:   {hash}");
    println!("  Decoded:   ({dlat:.6}, {dlon:.6})");
    println!("  Error:     {:.6}° lat, {:.6}° lon", (lat - dlat).abs(), (lon - dlon).abs());

    // Neighbors
    println!("\n--- Neighbors of '{}' ---", &hash[..5]);
    let nbrs = neighbors(&hash[..5]);
    let labels = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    for (label, nbr) in labels.iter().zip(&nbrs) {
        println!("  {label:>2}: {nbr}");
    }

    // Prefix sharing = proximity
    println!("\n--- Proximity by Prefix ---");
    let nearby_point = encode(40.6895, -74.0440, 9); // very close
    let far_point = encode(48.8584, 2.2945, 9);      // Paris
    println!("  Liberty:  {hash}");
    println!("  Near:     {nearby_point}");
    println!("  Paris:    {far_point}");

    let shared_near = hash.chars().zip(nearby_point.chars()).take_while(|(a, b)| a == b).count();
    let shared_far = hash.chars().zip(far_point.chars()).take_while(|(a, b)| a == b).count();
    println!("  Shared prefix with near: {shared_near} chars");
    println!("  Shared prefix with far: {shared_far} chars");

    // Spatial index demo
    println!("\n--- Spatial Index (NYC restaurants) ---");
    let mut idx = GeoIndex::new(7);
    // Some NYC restaurants (approximate coords)
    idx.insert("Joe's Pizza",       40.7308, -73.9973);
    idx.insert("Le Bernardin",      40.7614, -73.9818);
    idx.insert("Peter Luger",       40.7099, -73.9624);
    idx.insert("Katz's Deli",       40.7223, -73.9874);
    idx.insert("Di Fara Pizza",     40.6250, -73.9616);

    let results = idx.nearby(40.7250, -73.9900);
    println!("  Nearby (40.725, -73.990):");
    for (name, lat, lon, dist) in &results {
        println!("    {name:<20} ({lat:.4}, {lon:.4})  {dist:.2} km");
    }
}
