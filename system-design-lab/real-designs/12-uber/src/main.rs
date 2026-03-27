#![allow(dead_code, unused_variables, unused_imports)]
//! # Uber (Ride-Sharing) - Mini Implementation
//!
//! Demonstrates:
//! - Driver location tracking with geospatial index
//! - Ride matching algorithm
//! - ETA calculation
//! - Surge pricing
//! - Ride state machine
//!
//! Run: cargo run -p uber

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn instant_now() -> Instant {
    Instant::now()
}

fn default_option_instant() -> Option<Instant> {
    None
}

// =============================================================================
// Core Types
// =============================================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct GeoPoint {
    lat: f64,
    lon: f64,
}

impl GeoPoint {
    fn distance_km(&self, other: &GeoPoint) -> f64 {
        let r = 6371.0;
        let dlat = (other.lat - self.lat).to_radians();
        let dlon = (other.lon - self.lon).to_radians();
        let a = (dlat / 2.0).sin().powi(2)
            + self.lat.to_radians().cos()
                * other.lat.to_radians().cos()
                * (dlon / 2.0).sin().powi(2);
        r * 2.0 * a.sqrt().asin()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Driver {
    id: String,
    name: String,
    location: GeoPoint,
    status: DriverStatus,
    rating: f64,
    vehicle_type: VehicleType,
    current_ride: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
enum DriverStatus {
    Offline,
    Available,
    EnRoute, // Going to pickup
    OnTrip,  // Carrying passenger
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum VehicleType {
    UberX,
    UberXL,
    UberBlack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Rider {
    id: String,
    name: String,
    location: GeoPoint,
    rating: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RideRequest {
    id: String,
    rider_id: String,
    pickup: GeoPoint,
    dropoff: GeoPoint,
    vehicle_type: VehicleType,
    #[serde(skip, default = "instant_now")]
    created_at: Instant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Ride {
    id: String,
    request: RideRequest,
    driver_id: String,
    status: RideStatus,
    price_estimate: f64,
    surge_multiplier: f64,
    eta_pickup: Duration,
    eta_dropoff: Duration,
    #[serde(skip, default = "default_option_instant")]
    started_at: Option<Instant>,
    #[serde(skip, default = "default_option_instant")]
    completed_at: Option<Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
enum RideStatus {
    Requested,
    Matched,
    DriverEnRoute,
    Arrived,
    InProgress,
    Completed,
    Cancelled,
}

// =============================================================================
// Geospatial Index (QuadTree-like Grid)
// =============================================================================

struct DriverIndex {
    cells: DashMap<String, Vec<String>>,   // cell_key -> driver_ids
    driver_cells: DashMap<String, String>, // driver_id -> cell_key
    drivers: DashMap<String, Driver>,
    cell_size: f64, // degrees
}

impl DriverIndex {
    fn new() -> Self {
        Self {
            cells: DashMap::new(),
            driver_cells: DashMap::new(),
            drivers: DashMap::new(),
            cell_size: 0.01, // ~1km cells
        }
    }

    fn cell_key(&self, point: &GeoPoint) -> String {
        format!(
            "{}:{}",
            (point.lat / self.cell_size).floor() as i32,
            (point.lon / self.cell_size).floor() as i32
        )
    }

    fn update_driver(&self, driver: Driver) {
        let driver_id = driver.id.clone();
        let new_cell = self.cell_key(&driver.location);

        // Remove from old cell
        if let Some(old_cell) = self.driver_cells.get(&driver_id) {
            if *old_cell != new_cell {
                if let Some(mut cell) = self.cells.get_mut(&*old_cell) {
                    cell.retain(|id| id != &driver_id);
                }
            }
        }

        // Add to new cell
        self.cells
            .entry(new_cell.clone())
            .or_default()
            .push(driver_id.clone());

        self.driver_cells.insert(driver_id.clone(), new_cell);
        self.drivers.insert(driver_id, driver);
    }

    fn find_nearby_available(
        &self,
        location: &GeoPoint,
        radius_km: f64,
        limit: usize,
    ) -> Vec<Driver> {
        let center_lat = (location.lat / self.cell_size).floor() as i32;
        let center_lon = (location.lon / self.cell_size).floor() as i32;
        let cell_radius = (radius_km / (111.0 * self.cell_size)).ceil() as i32 + 1;

        let mut candidates: Vec<(f64, Driver)> = Vec::new();

        for dlat in -cell_radius..=cell_radius {
            for dlon in -cell_radius..=cell_radius {
                let cell_key = format!("{}:{}", center_lat + dlat, center_lon + dlon);
                if let Some(driver_ids) = self.cells.get(&cell_key) {
                    for driver_id in driver_ids.iter() {
                        if let Some(driver) = self.drivers.get(driver_id) {
                            if driver.status == DriverStatus::Available {
                                let dist = location.distance_km(&driver.location);
                                if dist <= radius_km {
                                    candidates.push((dist, driver.clone()));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Sort by distance and return
        candidates.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        candidates.into_iter().take(limit).map(|(_, d)| d).collect()
    }
}

// =============================================================================
// Pricing Engine
// =============================================================================

struct PricingEngine {
    base_fare: DashMap<VehicleType, f64>,
    per_km: DashMap<VehicleType, f64>,
    per_minute: DashMap<VehicleType, f64>,
    // Demand tracking for surge
    demand_count: DashMap<String, AtomicU64>, // cell_key -> request count
    supply_count: DashMap<String, AtomicU64>, // cell_key -> available drivers
}

impl PricingEngine {
    fn new() -> Self {
        let engine = Self {
            base_fare: DashMap::new(),
            per_km: DashMap::new(),
            per_minute: DashMap::new(),
            demand_count: DashMap::new(),
            supply_count: DashMap::new(),
        };

        // Set base prices
        engine.base_fare.insert(VehicleType::UberX, 2.50);
        engine.base_fare.insert(VehicleType::UberXL, 4.00);
        engine.base_fare.insert(VehicleType::UberBlack, 7.00);

        engine.per_km.insert(VehicleType::UberX, 1.20);
        engine.per_km.insert(VehicleType::UberXL, 1.80);
        engine.per_km.insert(VehicleType::UberBlack, 3.50);

        engine.per_minute.insert(VehicleType::UberX, 0.20);
        engine.per_minute.insert(VehicleType::UberXL, 0.30);
        engine.per_minute.insert(VehicleType::UberBlack, 0.50);

        engine
    }

    fn calculate_surge(&self, cell_key: &str) -> f64 {
        let demand = self
            .demand_count
            .get(cell_key)
            .map(|d| d.load(Ordering::SeqCst))
            .unwrap_or(0);

        let supply = self
            .supply_count
            .get(cell_key)
            .map(|s| s.load(Ordering::SeqCst))
            .unwrap_or(1)
            .max(1);

        let ratio = demand as f64 / supply as f64;

        // Surge tiers
        if ratio > 3.0 {
            2.5
        } else if ratio > 2.0 {
            1.75
        } else if ratio > 1.5 {
            1.25
        } else {
            1.0
        }
    }

    fn estimate_price(
        &self,
        vehicle_type: VehicleType,
        distance_km: f64,
        duration_min: f64,
        surge: f64,
    ) -> f64 {
        let base = self
            .base_fare
            .get(&vehicle_type)
            .map(|v| *v)
            .unwrap_or(2.50);
        let km_cost = self.per_km.get(&vehicle_type).map(|v| *v).unwrap_or(1.20) * distance_km;
        let time_cost = self
            .per_minute
            .get(&vehicle_type)
            .map(|v| *v)
            .unwrap_or(0.20)
            * duration_min;

        (base + km_cost + time_cost) * surge
    }

    fn record_demand(&self, cell_key: &str) {
        self.demand_count
            .entry(cell_key.to_string())
            .or_insert(AtomicU64::new(0))
            .fetch_add(1, Ordering::SeqCst);
    }

    fn update_supply(&self, cell_key: &str, available: u64) {
        self.supply_count
            .entry(cell_key.to_string())
            .or_insert(AtomicU64::new(0))
            .store(available, Ordering::SeqCst);
    }
}

// =============================================================================
// Ride Service
// =============================================================================

struct RideService {
    driver_index: Arc<DriverIndex>,
    pricing: Arc<PricingEngine>,
    rides: DashMap<String, Ride>,
    ride_counter: AtomicU64,
}

impl RideService {
    fn new() -> Self {
        Self {
            driver_index: Arc::new(DriverIndex::new()),
            pricing: Arc::new(PricingEngine::new()),
            rides: DashMap::new(),
            ride_counter: AtomicU64::new(0),
        }
    }

    fn register_driver(&self, driver: Driver) {
        self.driver_index.update_driver(driver);
    }

    fn update_driver_location(&self, driver_id: &str, location: GeoPoint) {
        if let Some(mut driver) = self.driver_index.drivers.get_mut(driver_id) {
            driver.location = location;
            self.driver_index.update_driver(driver.clone());
        }
    }

    fn request_ride(
        &self,
        rider: &Rider,
        pickup: GeoPoint,
        dropoff: GeoPoint,
        vehicle_type: VehicleType,
    ) -> Ride {
        let ride_id = format!("ride_{}", self.ride_counter.fetch_add(1, Ordering::SeqCst));

        let request = RideRequest {
            id: ride_id.clone(),
            rider_id: rider.id.clone(),
            pickup,
            dropoff,
            vehicle_type,
            created_at: Instant::now(),
        };

        // Find nearest driver
        let nearby = self.driver_index.find_nearby_available(&pickup, 10.0, 5);

        let (driver_id, eta_pickup) = if let Some(driver) = nearby.first() {
            let eta = self.calculate_eta(&driver.location, &pickup);
            (driver.id.clone(), eta)
        } else {
            ("no_driver".to_string(), Duration::from_secs(999))
        };

        // Calculate pricing
        let cell_key = self.driver_index.cell_key(&pickup);
        self.pricing.record_demand(&cell_key);
        let surge = self.pricing.calculate_surge(&cell_key);

        let distance_km = pickup.distance_km(&dropoff);
        let duration_min = distance_km / 0.5; // Assume 30 km/h average
        let price = self
            .pricing
            .estimate_price(vehicle_type, distance_km, duration_min, surge);

        let ride = Ride {
            id: ride_id.clone(),
            request,
            driver_id: driver_id.clone(),
            status: RideStatus::Matched,
            price_estimate: price,
            surge_multiplier: surge,
            eta_pickup,
            eta_dropoff: Duration::from_secs((distance_km * 2.0 * 60.0) as u64),
            started_at: None,
            completed_at: None,
        };

        // Update driver status
        if let Some(mut driver) = self.driver_index.drivers.get_mut(&driver_id) {
            driver.status = DriverStatus::EnRoute;
            driver.current_ride = Some(ride_id.clone());
        }

        self.rides.insert(ride_id, ride.clone());
        ride
    }

    fn calculate_eta(&self, from: &GeoPoint, to: &GeoPoint) -> Duration {
        let distance_km = from.distance_km(to);
        // Assume 25 km/h in city traffic
        Duration::from_secs((distance_km * 60.0 * 60.0 / 25.0) as u64)
    }

    fn start_ride(&self, ride_id: &str) -> Option<Ride> {
        let mut ride = self.rides.get_mut(ride_id)?;
        ride.status = RideStatus::InProgress;
        ride.started_at = Some(Instant::now());

        if let Some(mut driver) = self.driver_index.drivers.get_mut(&ride.driver_id) {
            driver.status = DriverStatus::OnTrip;
        }

        Some(ride.clone())
    }

    fn complete_ride(&self, ride_id: &str) -> Option<Ride> {
        let mut ride = self.rides.get_mut(ride_id)?;
        ride.status = RideStatus::Completed;
        ride.completed_at = Some(Instant::now());

        if let Some(mut driver) = self.driver_index.drivers.get_mut(&ride.driver_id) {
            driver.status = DriverStatus::Available;
            driver.current_ride = None;
        }

        Some(ride.clone())
    }

    fn get_price_estimates(
        &self,
        pickup: GeoPoint,
        dropoff: GeoPoint,
    ) -> Vec<(VehicleType, f64, f64)> {
        let distance_km = pickup.distance_km(&dropoff);
        let duration_min = distance_km / 0.5;
        let cell_key = self.driver_index.cell_key(&pickup);
        let surge = self.pricing.calculate_surge(&cell_key);

        vec![
            VehicleType::UberX,
            VehicleType::UberXL,
            VehicleType::UberBlack,
        ]
        .into_iter()
        .map(|vt| {
            let price = self
                .pricing
                .estimate_price(vt, distance_km, duration_min, surge);
            (vt, price, surge)
        })
        .collect()
    }
}

// =============================================================================
// Main Demo
// =============================================================================

fn main() {
    println!("=== Uber (Ride-Sharing) Demo ===\n");

    let service = RideService::new();

    // Register drivers
    println!("\n  ═══ Registering Drivers ═══");
    let drivers = vec![
        Driver {
            id: "d1".to_string(),
            name: "John".to_string(),
            location: GeoPoint {
                lat: 40.7128,
                lon: -74.0060,
            },
            status: DriverStatus::Available,
            rating: 4.8,
            vehicle_type: VehicleType::UberX,
            current_ride: None,
        },
        Driver {
            id: "d2".to_string(),
            name: "Sarah".to_string(),
            location: GeoPoint {
                lat: 40.7150,
                lon: -74.0080,
            },
            status: DriverStatus::Available,
            rating: 4.9,
            vehicle_type: VehicleType::UberXL,
            current_ride: None,
        },
        Driver {
            id: "d3".to_string(),
            name: "Mike".to_string(),
            location: GeoPoint {
                lat: 40.7200,
                lon: -74.0100,
            },
            status: DriverStatus::OnTrip, // Already on a ride
            rating: 4.7,
            vehicle_type: VehicleType::UberBlack,
            current_ride: Some("existing_ride".to_string()),
        },
    ];

    for driver in drivers {
        println!(
            "Registered: {} ({:?}, ⭐{:.1})",
            driver.name, driver.vehicle_type, driver.rating
        );
        service.register_driver(driver);
    }
    println!();

    // Create rider
    let rider = Rider {
        id: "r1".to_string(),
        name: "Alice".to_string(),
        location: GeoPoint {
            lat: 40.7135,
            lon: -74.0070,
        },
        rating: 4.9,
    };

    // Get price estimates
    println!("\n  ═══ Price Estimates ═══");
    let pickup = GeoPoint {
        lat: 40.7135,
        lon: -74.0070,
    };
    let dropoff = GeoPoint {
        lat: 40.7580,
        lon: -73.9855,
    }; // Times Square

    let distance = pickup.distance_km(&dropoff);
    println!("Trip: {:.1} km\n", distance);

    let estimates = service.get_price_estimates(pickup, dropoff);
    for (vt, price, surge) in estimates {
        let surge_str = if surge > 1.0 {
            format!(" (⚡{:.1}x surge)", surge)
        } else {
            String::new()
        };
        println!("{:?}: ${:.2}{}", vt, price, surge_str);
    }
    println!();

    // Request ride
    println!("\n  ═══ Requesting Ride ═══");
    let ride = service.request_ride(&rider, pickup, dropoff, VehicleType::UberX);
    println!("Ride {} requested", ride.id);
    println!("Driver assigned: {}", ride.driver_id);
    println!("ETA: {:?}", ride.eta_pickup);
    println!("Price estimate: ${:.2}", ride.price_estimate);
    if ride.surge_multiplier > 1.0 {
        println!("Surge: {:.1}x", ride.surge_multiplier);
    }
    println!();

    // Simulate driver arriving and starting ride
    println!("\n  ═══ Ride Progress ═══");
    println!("Driver en route to pickup...");

    let started = service.start_ride(&ride.id).unwrap();
    println!("Ride started! Status: {:?}", started.status);

    let completed = service.complete_ride(&ride.id).unwrap();
    println!("Ride completed! Status: {:?}", completed.status);

    // Show driver status after ride
    println!("\n--- Driver Status After Ride ---");
    if let Some(driver) = service.driver_index.drivers.get("d1") {
        println!(
            "{}: {:?} (current_ride: {:?})",
            driver.name, driver.status, driver.current_ride
        );
    }

    // Find available drivers
    println!("\n--- Available Drivers Nearby ---");
    let available = service.driver_index.find_nearby_available(&pickup, 5.0, 10);
    println!("{} available within 5km:", available.len());
    for d in available {
        let dist = pickup.distance_km(&d.location);
        println!("  {} ({:?}) - {:.2}km away", d.name, d.vehicle_type, dist);
    }

    println!("\n=== Key Concepts ===");
    println!("1. Geo-Index: Grid-based driver location tracking");
    println!("2. Matching: Find nearest available driver");
    println!("3. ETA: Distance-based time estimate");
    println!("4. Surge: demand/supply ratio pricing");
    println!("5. State Machine: Requested->Matched->EnRoute->Started->Completed");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_nearby_drivers() {
        let index = DriverIndex::new();

        index.update_driver(Driver {
            id: "d1".to_string(),
            name: "D1".to_string(),
            location: GeoPoint {
                lat: 40.7128,
                lon: -74.0060,
            },
            status: DriverStatus::Available,
            rating: 4.5,
            vehicle_type: VehicleType::UberX,
            current_ride: None,
        });

        let nearby = index.find_nearby_available(
            &GeoPoint {
                lat: 40.7130,
                lon: -74.0062,
            },
            1.0,
            10,
        );

        assert_eq!(nearby.len(), 1);
    }

    #[test]
    fn test_surge_pricing() {
        let pricing = PricingEngine::new();

        // Record high demand
        for _ in 0..10 {
            pricing.record_demand("cell_1");
        }
        pricing.update_supply("cell_1", 2);

        let surge = pricing.calculate_surge("cell_1");
        assert!(surge > 1.0);
    }
}
