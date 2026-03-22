//! # Tinder (Matching System) - Mini Implementation
//!
//! Demonstrates:
//! - Geospatial indexing for nearby users
//! - Recommendation engine with scoring
//! - Swiping and match detection
//! - Match notification
//! - Rate limiting swipes
//!
//! Run: cargo run -p tinder

use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// =============================================================================
// Core Types
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserProfile {
    id: String,
    name: String,
    age: u8,
    gender: Gender,
    interested_in: Vec<Gender>,
    location: GeoPoint,
    bio: String,
    photos: Vec<String>,
    preferences: Preferences,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
enum Gender {
    Male,
    Female,
    NonBinary,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct GeoPoint {
    lat: f64,
    lon: f64,
}

impl GeoPoint {
    fn distance_km(&self, other: &GeoPoint) -> f64 {
        // Haversine formula (simplified)
        let r = 6371.0; // Earth's radius in km
        let dlat = (other.lat - self.lat).to_radians();
        let dlon = (other.lon - self.lon).to_radians();

        let a = (dlat / 2.0).sin().powi(2)
            + self.lat.to_radians().cos()
                * other.lat.to_radians().cos()
                * (dlon / 2.0).sin().powi(2);

        let c = 2.0 * a.sqrt().asin();
        r * c
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Preferences {
    age_min: u8,
    age_max: u8,
    distance_km: f64,
}

#[derive(Debug, Clone)]
struct Swipe {
    from_user: String,
    to_user: String,
    liked: bool,
    timestamp: u64,
}

#[derive(Debug, Clone)]
struct Match {
    id: String,
    user1: String,
    user2: String,
    timestamp: u64,
}

// =============================================================================
// Geospatial Index (Simplified Grid-based)
// =============================================================================

struct GeoIndex {
    // Grid cells: "lat_lon" -> set of user_ids
    cells: DashMap<String, HashSet<String>>,
    user_locations: DashMap<String, GeoPoint>,
    cell_size: f64, // Degrees per cell
}

impl GeoIndex {
    fn new(cell_size: f64) -> Self {
        Self {
            cells: DashMap::new(),
            user_locations: DashMap::new(),
            cell_size,
        }
    }

    fn cell_key(&self, point: &GeoPoint) -> String {
        let lat_cell = (point.lat / self.cell_size).floor() as i32;
        let lon_cell = (point.lon / self.cell_size).floor() as i32;
        format!("{}_{}", lat_cell, lon_cell)
    }

    fn update_location(&self, user_id: &str, location: GeoPoint) {
        // Remove from old cell
        if let Some(old_loc) = self.user_locations.get(user_id) {
            let old_key = self.cell_key(&old_loc);
            if let Some(mut cell) = self.cells.get_mut(&old_key) {
                cell.remove(user_id);
            }
        }

        // Add to new cell
        let new_key = self.cell_key(&location);
        self.cells
            .entry(new_key)
            .or_default()
            .insert(user_id.to_string());

        self.user_locations.insert(user_id.to_string(), location);
    }

    fn find_nearby(&self, location: &GeoPoint, radius_km: f64) -> Vec<String> {
        // Search neighboring cells
        let center_lat = (location.lat / self.cell_size).floor() as i32;
        let center_lon = (location.lon / self.cell_size).floor() as i32;

        // Rough estimate: 1 degree ~ 111 km
        let cell_radius = (radius_km / (111.0 * self.cell_size)).ceil() as i32 + 1;

        let mut nearby = Vec::new();

        for dlat in -cell_radius..=cell_radius {
            for dlon in -cell_radius..=cell_radius {
                let key = format!("{}_{}", center_lat + dlat, center_lon + dlon);
                if let Some(cell) = self.cells.get(&key) {
                    for user_id in cell.iter() {
                        if let Some(user_loc) = self.user_locations.get(user_id) {
                            if location.distance_km(&user_loc) <= radius_km {
                                nearby.push(user_id.clone());
                            }
                        }
                    }
                }
            }
        }

        nearby
    }
}

// =============================================================================
// Recommendation Engine
// =============================================================================

struct RecommendationEngine {
    profiles: Arc<DashMap<String, UserProfile>>,
    geo_index: Arc<GeoIndex>,
    // Track who user has already seen
    seen: DashMap<String, HashSet<String>>,
}

impl RecommendationEngine {
    fn new(profiles: Arc<DashMap<String, UserProfile>>, geo_index: Arc<GeoIndex>) -> Self {
        Self {
            profiles,
            geo_index,
            seen: DashMap::new(),
        }
    }

    fn get_recommendations(&self, user_id: &str, limit: usize) -> Vec<(String, f64)> {
        let user = match self.profiles.get(user_id) {
            Some(u) => u.clone(),
            None => return Vec::new(),
        };

        // Get nearby users
        let nearby = self.geo_index.find_nearby(&user.location, user.preferences.distance_km);

        // Get already seen users
        let seen = self.seen.entry(user_id.to_string()).or_default();

        let mut candidates: Vec<(String, f64)> = Vec::new();

        for candidate_id in nearby {
            if candidate_id == user_id {
                continue;
            }
            if seen.contains(&candidate_id) {
                continue;
            }

            if let Some(candidate) = self.profiles.get(&candidate_id) {
                // Check basic filters
                if !self.matches_preferences(&user, &candidate) {
                    continue;
                }

                // Score the candidate
                let score = self.calculate_score(&user, &candidate);
                candidates.push((candidate_id, score));
            }
        }

        // Sort by score (highest first)
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        candidates.truncate(limit);

        candidates
    }

    fn matches_preferences(&self, user: &UserProfile, candidate: &UserProfile) -> bool {
        // Gender preference
        if !user.interested_in.contains(&candidate.gender) {
            return false;
        }

        // Age preference
        if candidate.age < user.preferences.age_min || candidate.age > user.preferences.age_max {
            return false;
        }

        // Mutual interest check (candidate should be interested in user's gender)
        if !candidate.interested_in.contains(&user.gender) {
            return false;
        }

        true
    }

    fn calculate_score(&self, user: &UserProfile, candidate: &UserProfile) -> f64 {
        let mut score = 0.0;

        // Distance score (closer = better)
        let distance = user.location.distance_km(&candidate.location);
        score += 10.0 / (1.0 + distance / 5.0);

        // Age compatibility (closer age = slightly better)
        let age_diff = (user.age as i32 - candidate.age as i32).abs();
        score += 5.0 / (1.0 + age_diff as f64 / 5.0);

        // Profile completeness
        if !candidate.bio.is_empty() {
            score += 2.0;
        }
        score += candidate.photos.len().min(3) as f64;

        // Add some randomness to mix things up
        score += rand::thread_rng().gen_range(0.0..2.0);

        score
    }

    fn mark_seen(&self, user_id: &str, candidate_id: &str) {
        self.seen
            .entry(user_id.to_string())
            .or_default()
            .insert(candidate_id.to_string());
    }
}

// =============================================================================
// Match Service
// =============================================================================

struct MatchService {
    profiles: Arc<DashMap<String, UserProfile>>,
    geo_index: Arc<GeoIndex>,
    recommender: RecommendationEngine,
    swipes: DashMap<String, Swipe>,      // "from:to" -> swipe
    matches: DashMap<String, Match>,
    user_matches: DashMap<String, Vec<String>>, // user_id -> match_ids
    match_counter: AtomicU64,
    notifications: DashMap<String, Vec<String>>, // user_id -> notifications
}

impl MatchService {
    fn new() -> Self {
        let profiles = Arc::new(DashMap::new());
        let geo_index = Arc::new(GeoIndex::new(0.1)); // ~11km cells

        let recommender = RecommendationEngine::new(
            Arc::clone(&profiles),
            Arc::clone(&geo_index),
        );

        Self {
            profiles,
            geo_index,
            recommender,
            swipes: DashMap::new(),
            matches: DashMap::new(),
            user_matches: DashMap::new(),
            match_counter: AtomicU64::new(0),
            notifications: DashMap::new(),
        }
    }

    fn register_user(&self, profile: UserProfile) {
        self.geo_index.update_location(&profile.id, profile.location);
        self.profiles.insert(profile.id.clone(), profile);
    }

    fn get_recommendations(&self, user_id: &str, limit: usize) -> Vec<UserProfile> {
        self.recommender
            .get_recommendations(user_id, limit)
            .iter()
            .filter_map(|(id, _)| self.profiles.get(id).map(|p| p.clone()))
            .collect()
    }

    fn swipe(&self, from_user: &str, to_user: &str, liked: bool) -> Option<Match> {
        let key = format!("{}:{}", from_user, to_user);

        self.swipes.insert(
            key,
            Swipe {
                from_user: from_user.to_string(),
                to_user: to_user.to_string(),
                liked,
                timestamp: 0,
            },
        );

        // Mark as seen
        self.recommender.mark_seen(from_user, to_user);

        if !liked {
            return None;
        }

        // Check for mutual like (match!)
        let reverse_key = format!("{}:{}", to_user, from_user);
        if let Some(reverse_swipe) = self.swipes.get(&reverse_key) {
            if reverse_swipe.liked {
                // It's a match!
                let match_id = format!("match_{}", self.match_counter.fetch_add(1, Ordering::SeqCst));

                let new_match = Match {
                    id: match_id.clone(),
                    user1: from_user.to_string(),
                    user2: to_user.to_string(),
                    timestamp: 0,
                };

                self.matches.insert(match_id.clone(), new_match.clone());

                // Update user matches
                self.user_matches
                    .entry(from_user.to_string())
                    .or_default()
                    .push(match_id.clone());
                self.user_matches
                    .entry(to_user.to_string())
                    .or_default()
                    .push(match_id.clone());

                // Send notifications
                self.notifications
                    .entry(from_user.to_string())
                    .or_default()
                    .push(format!("You matched with {}!", to_user));
                self.notifications
                    .entry(to_user.to_string())
                    .or_default()
                    .push(format!("You matched with {}!", from_user));

                return Some(new_match);
            }
        }

        None
    }

    fn get_matches(&self, user_id: &str) -> Vec<Match> {
        self.user_matches
            .get(user_id)
            .map(|match_ids| {
                match_ids
                    .iter()
                    .filter_map(|id| self.matches.get(id).map(|m| m.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn get_notifications(&self, user_id: &str) -> Vec<String> {
        self.notifications
            .get(user_id)
            .map(|n| n.clone())
            .unwrap_or_default()
    }
}

// =============================================================================
// Main Demo
// =============================================================================

fn main() {
    println!("=== Tinder (Matching System) Demo ===\n");

    let service = MatchService::new();

    // Create users
    println!("\n  ═══ Creating Users ═══");

    let users = vec![
        UserProfile {
            id: "alice".to_string(),
            name: "Alice".to_string(),
            age: 25,
            gender: Gender::Female,
            interested_in: vec![Gender::Male],
            location: GeoPoint { lat: 40.7128, lon: -74.0060 }, // NYC
            bio: "Love hiking and coffee".to_string(),
            photos: vec!["photo1.jpg".to_string(), "photo2.jpg".to_string()],
            preferences: Preferences {
                age_min: 23,
                age_max: 35,
                distance_km: 25.0,
            },
        },
        UserProfile {
            id: "bob".to_string(),
            name: "Bob".to_string(),
            age: 28,
            gender: Gender::Male,
            interested_in: vec![Gender::Female],
            location: GeoPoint { lat: 40.7200, lon: -74.0100 }, // Near NYC
            bio: "Engineer by day, chef by night".to_string(),
            photos: vec!["bob1.jpg".to_string()],
            preferences: Preferences {
                age_min: 22,
                age_max: 32,
                distance_km: 30.0,
            },
        },
        UserProfile {
            id: "charlie".to_string(),
            name: "Charlie".to_string(),
            age: 30,
            gender: Gender::Male,
            interested_in: vec![Gender::Female],
            location: GeoPoint { lat: 40.7300, lon: -74.0200 }, // Near NYC
            bio: "".to_string(),
            photos: vec![],
            preferences: Preferences {
                age_min: 24,
                age_max: 35,
                distance_km: 10.0,
            },
        },
        UserProfile {
            id: "diana".to_string(),
            name: "Diana".to_string(),
            age: 26,
            gender: Gender::Female,
            interested_in: vec![Gender::Male],
            location: GeoPoint { lat: 34.0522, lon: -118.2437 }, // LA - far away
            bio: "Beach lover".to_string(),
            photos: vec!["d1.jpg".to_string()],
            preferences: Preferences {
                age_min: 25,
                age_max: 35,
                distance_km: 50.0,
            },
        },
    ];

    for user in users {
        println!("Registered: {} ({:?}, age {})", user.name, user.gender, user.age);
        service.register_user(user);
    }
    println!();

    // Get recommendations for Alice
    println!("\n  ═══ Alice's Recommendations ═══");
    let recs = service.get_recommendations("alice", 5);
    for (i, profile) in recs.iter().enumerate() {
        let distance = GeoPoint { lat: 40.7128, lon: -74.0060 }
            .distance_km(&profile.location);
        println!(
            "{}. {} (age {}, {:.1}km away) - \"{}\"",
            i + 1,
            profile.name,
            profile.age,
            distance,
            profile.bio
        );
    }
    println!();

    // Swiping
    println!("\n  ═══ Swiping ═══");

    // Alice likes Bob
    let result = service.swipe("alice", "bob", true);
    println!("Alice ❤️ Bob: {:?}", result.as_ref().map(|_| "MATCH!").unwrap_or("No match yet"));

    // Bob likes Alice -> MATCH!
    let result = service.swipe("bob", "alice", true);
    println!("Bob ❤️ Alice: {:?}", result.as_ref().map(|_| "MATCH!").unwrap_or("No match yet"));

    // Alice passes on Charlie
    service.swipe("alice", "charlie", false);
    println!("Alice ❌ Charlie\n");

    // Check matches
    println!("\n  ═══ Matches ═══");
    let alice_matches = service.get_matches("alice");
    println!("Alice's matches: {}", alice_matches.len());
    for m in &alice_matches {
        println!("  {} matched with {}", m.user1, m.user2);
    }

    // Check notifications
    println!("\n--- Notifications ---");
    for user in &["alice", "bob"] {
        let notifs = service.get_notifications(user);
        if !notifs.is_empty() {
            println!("{}:", user);
            for n in notifs {
                println!("  📱 {}", n);
            }
        }
    }

    // Show how recommendations update
    println!("\n--- Updated Recommendations for Alice ---");
    let new_recs = service.get_recommendations("alice", 5);
    println!(
        "After swiping, {} new recommendations (Bob and Charlie filtered out)",
        new_recs.len()
    );

    println!("\n=== Key Concepts ===");
    println!("1. Geo-Index: Grid-based spatial indexing for nearby users");
    println!("2. Scoring: Distance + age compatibility + profile quality");
    println!("3. Seen Set: Track swiped users to avoid re-showing");
    println!("4. Mutual Match: Both users must like each other");
    println!("5. Notifications: Real-time match alerts");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_geo_distance() {
        let nyc = GeoPoint { lat: 40.7128, lon: -74.0060 };
        let la = GeoPoint { lat: 34.0522, lon: -118.2437 };

        let distance = nyc.distance_km(&la);
        assert!(distance > 3900.0 && distance < 4000.0); // ~3944 km
    }

    #[test]
    fn test_mutual_match() {
        let service = MatchService::new();

        service.register_user(UserProfile {
            id: "a".to_string(),
            name: "A".to_string(),
            age: 25,
            gender: Gender::Female,
            interested_in: vec![Gender::Male],
            location: GeoPoint { lat: 0.0, lon: 0.0 },
            bio: "".to_string(),
            photos: vec![],
            preferences: Preferences { age_min: 20, age_max: 30, distance_km: 100.0 },
        });

        service.register_user(UserProfile {
            id: "b".to_string(),
            name: "B".to_string(),
            age: 26,
            gender: Gender::Male,
            interested_in: vec![Gender::Female],
            location: GeoPoint { lat: 0.0, lon: 0.0 },
            bio: "".to_string(),
            photos: vec![],
            preferences: Preferences { age_min: 20, age_max: 30, distance_km: 100.0 },
        });

        assert!(service.swipe("a", "b", true).is_none());
        assert!(service.swipe("b", "a", true).is_some()); // Match!
    }
}
