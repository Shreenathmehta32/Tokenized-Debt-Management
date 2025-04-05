#![allow(non_snake_case)]
#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, 
    log, symbol_short, Address, Env, 
    Symbol, String, Vec,
    // Removed Map as it was unused
};

// Asset status tracking
#[contracttype]
#[derive(Clone)]
pub struct AssetStats {
    pub total_assets: u64,            // Total number of registered assets
    pub active_leases: u64,           // Number of currently active leases
    pub completed_leases: u64,        // Number of completed leases
    pub disputed_leases: u64,         // Number of disputed leases
    pub total_value_locked: i128,     // Total XLM value locked in contracts
    pub total_earnings: i128,         // Total earnings generated from leases
}

// Key for accessing global stats - Shortened to 9 chars max
const ASSET_STS: Symbol = symbol_short!("ASSET_STS");

// Counter for unique asset IDs - Shortened to 9 chars max
const ASSET_CNT: Symbol = symbol_short!("ASSET_CNT");

// Counter for unique lease IDs - Shortened to 9 chars max
const LEASE_CNT: Symbol = symbol_short!("LEASE_CNT");

// Asset data structure
#[contracttype]
#[derive(Clone)]
pub struct Asset {
    pub asset_id: u64,                // Unique identifier for the asset
    pub owner: Address,               // Asset owner's address
    pub title: String,                // Asset title/name
    pub description: String,          // Asset description
    pub asset_value: i128,            // Asset value in XLM
    pub daily_rate: i128,             // Daily leasing rate in XLM
    pub available: bool,              // Availability status
    pub min_lease_days: u64,          // Minimum lease period in days
    pub max_lease_days: u64,          // Maximum lease period in days
    pub security_deposit: i128,       // Required security deposit in XLM
    pub created_at: u64,              // Asset registration timestamp
}

// Enum for mapping asset_id to Asset
#[contracttype]
pub enum AssetRegistry {
    Asset(u64)
}

// Lease data structure
#[contracttype]
#[derive(Clone)]
pub struct Lease {
    pub lease_id: u64,                // Unique identifier for the lease
    pub asset_id: u64,                // Associated asset ID
    pub lessor: Address,              // Asset owner's address
    pub lessee: Address,              // Lessee's address
    pub start_time: u64,              // Lease start timestamp
    pub end_time: u64,                // Lease end timestamp
    pub total_amount: i128,           // Total lease amount in XLM
    pub security_deposit: i128,       // Security deposit amount in XLM
    pub is_active: bool,              // Lease active status
    pub is_completed: bool,           // Lease completion status
    pub is_disputed: bool,            // Dispute status
    pub last_payment: u64,            // Last payment timestamp
    pub return_condition: String,     // Condition description upon return
    pub penalty_amount: i128,         // Penalty amount (if applicable)
    pub daily_rate: i128,             // Added daily_rate field which was missing
}

// Enum for mapping lease_id to Lease
#[contracttype]
pub enum LeaseRegistry {
    Lease(u64)
}

// User's assets and leases
#[contracttype]
#[derive(Clone)]
pub struct UserPortfolio {
    pub owned_assets: Vec<u64>,       // Asset IDs owned by user
    pub active_leases_as_lessor: Vec<u64>,  // Active leases as owner
    pub active_leases_as_lessee: Vec<u64>,  // Active leases as lessee
    pub completed_leases: Vec<u64>,   // Completed lease IDs
}

// Enum for mapping user address to UserPortfolio
#[contracttype]
pub enum UserRegistry {
    User(Address)
}

// Lease status enum
#[contracttype]
#[derive(Clone, PartialEq)]
pub enum LeaseStatus {
    Pending,
    Active,
    Completed,
    Disputed,
    Canceled,
}

#[contract]
pub struct AssetLeasingContract;

#[contractimpl]
impl AssetLeasingContract {
    // Register a new asset
    pub fn register_asset(
        env: Env,
        owner: Address,
        title: String,
        description: String,
        asset_value: i128,
        daily_rate: i128,
        min_lease_days: u64,
        max_lease_days: u64,
        security_deposit: i128,
    ) -> u64 {
        // Verify inputs
        if daily_rate <= 0 || asset_value <= 0 || security_deposit < 0 {
            log!(&env, "Invalid asset parameters");
            panic!("Invalid asset parameters");
        }

        // Authenticate owner
        owner.require_auth();

        // Get new asset_id
        let mut asset_count: u64 = env.storage().instance().get(&ASSET_CNT).unwrap_or(0);
        asset_count += 1;

        // Create asset
        let asset = Asset {
            asset_id: asset_count,
            owner: owner.clone(),
            title,
            description,
            asset_value,
            daily_rate,
            available: true,
            min_lease_days,
            max_lease_days,
            security_deposit,
            created_at: env.ledger().timestamp(),
        };

        // Store asset
        env.storage().instance().set(&AssetRegistry::Asset(asset_count), &asset);
        
        // Update stats
        let mut stats = Self::get_asset_stats(env.clone());
        stats.total_assets += 1;
        env.storage().instance().set(&ASSET_STS, &stats);
        
        // Update asset count
        env.storage().instance().set(&ASSET_CNT, &asset_count);
        
        // Update user portfolio
        let mut portfolio = Self::get_user_portfolio(env.clone(), owner.clone());
        portfolio.owned_assets.push_back(asset_count);
        env.storage().instance().set(&UserRegistry::User(owner), &portfolio);
        
        // Extend storage lifetime
        env.storage().instance().extend_ttl(10000, 10000);
        
        log!(&env, "Asset registered with ID: {}", asset_count);
        asset_count
    }

    // Create a lease proposal (by lessee)
    pub fn create_lease(
        env: Env,
        asset_id: u64,
        lessee: Address,
        days: u64,
    ) -> u64 {
        // Authenticate lessee
        lessee.require_auth();
        
        // Get asset
        let asset = Self::get_asset(env.clone(), asset_id);
        
        // Check availability
        if !asset.available {
            log!(&env, "Asset is not available for lease");
            panic!("Asset is not available for lease");
        }
        
        // Validate lease period
        if days < asset.min_lease_days || days > asset.max_lease_days {
            log!(&env, "Lease duration outside acceptable range");
            panic!("Lease duration outside acceptable range");
        }
        
        // Calculate total amount
        let total_amount = asset.daily_rate * days as i128;
        let security_deposit = asset.security_deposit;
        // Renamed to avoid unused variable warning
        let _total_required = total_amount + security_deposit;
        
        // Get new lease_id
        let mut lease_count: u64 = env.storage().instance().get(&LEASE_CNT).unwrap_or(0);
        lease_count += 1;
        
        // Get current time
        let current_time = env.ledger().timestamp();
        let end_time = current_time + (days * 24 * 60 * 60); // Convert days to seconds
        
        // Create lease
        let lease = Lease {
            lease_id: lease_count,
            asset_id,
            lessor: asset.owner.clone(),
            lessee: lessee.clone(),
            start_time: current_time,
            end_time,
            total_amount,
            security_deposit,
            is_active: false, // Requires approval
            is_completed: false,
            is_disputed: false,
            last_payment: 0,
            return_condition: String::from_str(&env, ""),
            penalty_amount: 0,
            daily_rate: asset.daily_rate, // Added daily_rate field
        };
        
        // Store lease
        env.storage().instance().set(&LeaseRegistry::Lease(lease_count), &lease);
        
        // Update asset availability
        let mut updated_asset = asset.clone();
        updated_asset.available = false;
        env.storage().instance().set(&AssetRegistry::Asset(asset_id), &updated_asset);
        
        // Update lease count
        env.storage().instance().set(&LEASE_CNT, &lease_count);
        
        // Update lessee portfolio
        let mut lessee_portfolio = Self::get_user_portfolio(env.clone(), lessee.clone());
        lessee_portfolio.active_leases_as_lessee.push_back(lease_count);
        env.storage().instance().set(&UserRegistry::User(lessee), &lessee_portfolio);
        
        // Update lessor portfolio
        let mut lessor_portfolio = Self::get_user_portfolio(env.clone(), asset.owner.clone());
        lessor_portfolio.active_leases_as_lessor.push_back(lease_count);
        env.storage().instance().set(&UserRegistry::User(asset.owner), &lessor_portfolio);
        
        // Extend storage lifetime
        env.storage().instance().extend_ttl(10000, 10000);
        
        log!(&env, "Lease created with ID: {}", lease_count);
        lease_count
    }

    // Approve and activate lease (by lessor/owner)
    pub fn approve_lease(env: Env, lease_id: u64, lessor: Address) -> bool {
        // Authenticate lessor
        lessor.require_auth();
        
        // Get lease
        let mut lease = Self::get_lease(env.clone(), lease_id);
        
        // Verify lessor is the asset owner
        if lease.lessor != lessor {
            log!(&env, "Only the asset owner can approve this lease");
            panic!("Only the asset owner can approve this lease");
        }
        
        // Verify lease isn't already active
        if lease.is_active {
            log!(&env, "Lease is already active");
            panic!("Lease is already active");
        }
        
        // Activate lease
        lease.is_active = true;
        lease.last_payment = env.ledger().timestamp();
        env.storage().instance().set(&LeaseRegistry::Lease(lease_id), &lease);
        
        // Update stats
        let mut stats = Self::get_asset_stats(env.clone());
        stats.active_leases += 1;
        stats.total_value_locked += lease.total_amount + lease.security_deposit;
        env.storage().instance().set(&ASSET_STS, &stats);
        
        // Extend storage lifetime
        env.storage().instance().extend_ttl(10000, 10000);
        
        log!(&env, "Lease {} approved and activated", lease_id);
        true
    }

    // Complete a lease (return asset)
    pub fn complete_lease(
        env: Env, 
        lease_id: u64, 
        lessee: Address, 
        return_condition: String,
        has_damages: bool
    ) -> bool {
        // Authenticate lessee
        lessee.require_auth();
        
        // Get lease
        let mut lease = Self::get_lease(env.clone(), lease_id);
        
        // Verify lessee
        if lease.lessee != lessee {
            log!(&env, "Only the lessee can complete this lease");
            panic!("Only the lessee can complete this lease");
        }
        
        // Verify lease is active
        if !lease.is_active || lease.is_completed {
            log!(&env, "Lease is not active or already completed");
            panic!("Lease is not active or already completed");
        }
        
        // Update lease status
        lease.is_active = false;
        lease.is_completed = true;
        lease.return_condition = return_condition;
        
        // Calculate penalties for damages or late return
        let current_time = env.ledger().timestamp();
        let penalty = if has_damages {
            // Apply damage penalty (25% of security deposit)
            lease.security_deposit / 4
        } else if current_time > lease.end_time {
            // Apply late return penalty (10% of daily rate per day)
            let days_late = (current_time - lease.end_time) / (24 * 60 * 60);
            lease.daily_rate / 10 * days_late as i128
        } else {
            0
        };
        
        lease.penalty_amount = penalty;
        
        // Store updated lease
        env.storage().instance().set(&LeaseRegistry::Lease(lease_id), &lease);
        
        // Make asset available again
        let mut asset = Self::get_asset(env.clone(), lease.asset_id);
        asset.available = true;
        env.storage().instance().set(&AssetRegistry::Asset(lease.asset_id), &asset);
        
        // Update stats
        let mut stats = Self::get_asset_stats(env.clone());
        stats.active_leases -= 1;
        stats.completed_leases += 1;
        stats.total_value_locked -= lease.total_amount + lease.security_deposit;
        stats.total_earnings += lease.total_amount + penalty;
        env.storage().instance().set(&ASSET_STS, &stats);
        
        // Update user portfolios
        Self::update_portfolios_on_completion(env.clone(), lease_id, lease.lessee.clone(), lease.lessor.clone());
        
        // Extend storage lifetime
        env.storage().instance().extend_ttl(10000, 10000);
        
        log!(&env, "Lease {} completed, penalty: {}", lease_id, penalty);
        true
    }

    // File a dispute for a lease
    pub fn file_dispute(
        env: Env,
        lease_id: u64,
        filer: Address,
        dispute_reason: String
    ) -> bool {
        // Authenticate filer
        filer.require_auth();
        
        // Get lease
        let mut lease = Self::get_lease(env.clone(), lease_id);
        
        // Verify filer is lessor or lessee
        if lease.lessor != filer && lease.lessee != filer {
            log!(&env, "Only the lessor or lessee can file a dispute");
            panic!("Only the lessor or lessee can file a dispute");
        }
        
        // Check lease status
        if !lease.is_active {
            log!(&env, "Can only dispute active leases");
            panic!("Can only dispute active leases");
        }
        
        // Mark as disputed
        lease.is_disputed = true;
        
        // Store dispute reason in return_condition field temporarily
        lease.return_condition = dispute_reason;
        
        // Store updated lease
        env.storage().instance().set(&LeaseRegistry::Lease(lease_id), &lease);
        
        // Update stats
        let mut stats = Self::get_asset_stats(env.clone());
        stats.disputed_leases += 1;
        env.storage().instance().set(&ASSET_STS, &stats);
        
        // Extend storage lifetime
        env.storage().instance().extend_ttl(10000, 10000);
        
        log!(&env, "Dispute filed for lease {}", lease_id);
        true
    }

    // Resolve a dispute (by admin or consensus)
    pub fn resolve_dispute(
        env: Env,
        lease_id: u64,
        admin: Address,
        _in_favor_of_lessor: bool, // Renamed with underscore to avoid unused variable warning
        penalty_percentage: u64
    ) -> bool {
        // Authenticate admin
        admin.require_auth();
        
        // Get lease
        let mut lease = Self::get_lease(env.clone(), lease_id);
        
        // Check if disputed
        if !lease.is_disputed {
            log!(&env, "Lease is not under dispute");
            panic!("Lease is not under dispute");
        }
        
        // Calculate penalty based on security deposit and percentage
        let penalty = lease.security_deposit * penalty_percentage as i128 / 100;
        lease.penalty_amount = penalty;
        
        // Resolve dispute
        lease.is_disputed = false;
        lease.is_active = false;
        lease.is_completed = true;
        
        // Store updated lease
        env.storage().instance().set(&LeaseRegistry::Lease(lease_id), &lease);
        
        // Make asset available again
        let mut asset = Self::get_asset(env.clone(), lease.asset_id);
        asset.available = true;
        env.storage().instance().set(&AssetRegistry::Asset(lease.asset_id), &asset);
        
        // Update stats
        let mut stats = Self::get_asset_stats(env.clone());
        stats.disputed_leases -= 1;
        stats.active_leases -= 1;
        stats.completed_leases += 1;
        stats.total_value_locked -= lease.total_amount + lease.security_deposit;
        stats.total_earnings += lease.total_amount + penalty;
        env.storage().instance().set(&ASSET_STS, &stats);
        
        // Update user portfolios
        Self::update_portfolios_on_completion(env.clone(), lease_id, lease.lessee.clone(), lease.lessor.clone());
        
        // Extend storage lifetime
        env.storage().instance().extend_ttl(10000, 10000);
        
        log!(&env, "Dispute resolved for lease {}, penalty: {}", lease_id, penalty);
        true
    }

    // Update asset details
    pub fn update_asset(
        env: Env,
        asset_id: u64,
        owner: Address,
        title: String,
        description: String,
        daily_rate: i128,
        available: bool,
        min_lease_days: u64,
        max_lease_days: u64,
    ) -> bool {
        // Authenticate owner
        owner.require_auth();
        
        // Get asset
        let mut asset = Self::get_asset(env.clone(), asset_id);
        
        // Verify owner
        if asset.owner != owner {
            log!(&env, "Only the asset owner can update this asset");
            panic!("Only the asset owner can update this asset");
        }
        
        // Update fields
        asset.title = title;
        asset.description = description;
        asset.daily_rate = daily_rate;
        asset.available = available;
        asset.min_lease_days = min_lease_days;
        asset.max_lease_days = max_lease_days;
        
        // Store updated asset
        env.storage().instance().set(&AssetRegistry::Asset(asset_id), &asset);
        
        // Extend storage lifetime
        env.storage().instance().extend_ttl(10000, 10000);
        
        log!(&env, "Asset {} updated", asset_id);
        true
    }

    // Get asset by ID
    pub fn get_asset(env: Env, asset_id: u64) -> Asset {
        env.storage().instance().get(&AssetRegistry::Asset(asset_id)).unwrap_or_else(|| {
            log!(&env, "Asset not found: {}", asset_id);
            panic!("Asset not found");
        })
    }

    // Get lease by ID
    pub fn get_lease(env: Env, lease_id: u64) -> Lease {
        env.storage().instance().get(&LeaseRegistry::Lease(lease_id)).unwrap_or_else(|| {
            log!(&env, "Lease not found: {}", lease_id);
            panic!("Lease not found");
        })
    }

    // Get user portfolio - changed parameter to take Address by value instead of reference
    pub fn get_user_portfolio(env: Env, user: Address) -> UserPortfolio {
        env.storage().instance().get(&UserRegistry::User(user.clone())).unwrap_or(UserPortfolio {
            owned_assets: Vec::new(&env),
            active_leases_as_lessor: Vec::new(&env),
            active_leases_as_lessee: Vec::new(&env),
            completed_leases: Vec::new(&env),
        })
    }

    // Get asset statistics
    pub fn get_asset_stats(env: Env) -> AssetStats {
        env.storage().instance().get(&ASSET_STS).unwrap_or(AssetStats {
            total_assets: 0,
            active_leases: 0,
            completed_leases: 0,
            disputed_leases: 0,
            total_value_locked: 0,
            total_earnings: 0,
        })
    }

    // Helper function to update user portfolios when a lease is completed
    // Changed to take Address by value instead of reference
    fn update_portfolios_on_completion(env: Env, lease_id: u64, lessee: Address, lessor: Address) {
        // Update lessee portfolio
        let mut lessee_portfolio = Self::get_user_portfolio(env.clone(), lessee.clone());
        let lessee_active = lessee_portfolio.active_leases_as_lessee.clone();
        let mut new_lessee_active = Vec::new(&env);
        
        // Remove lease from active leases - Fixed dereferencing
        for id in lessee_active.iter() {
            if id != lease_id {
                new_lessee_active.push_back(id);
            }
        }
        
        lessee_portfolio.active_leases_as_lessee = new_lessee_active;
        lessee_portfolio.completed_leases.push_back(lease_id);
        env.storage().instance().set(&UserRegistry::User(lessee), &lessee_portfolio);
        
        // Update lessor portfolio
        let mut lessor_portfolio = Self::get_user_portfolio(env.clone(), lessor.clone());
        let lessor_active = lessor_portfolio.active_leases_as_lessor.clone();
        let mut new_lessor_active = Vec::new(&env);
        
        // Remove lease from active leases - Fixed dereferencing
        for id in lessor_active.iter() {
            if id != lease_id {
                new_lessor_active.push_back(id);
            }
        }
        
        lessor_portfolio.active_leases_as_lessor = new_lessor_active;
        lessor_portfolio.completed_leases.push_back(lease_id);
        env.storage().instance().set(&UserRegistry::User(lessor), &lessor_portfolio);
    }

    // Get user's owned assets
    pub fn get_user_assets(env: Env, user: Address) -> Vec<Asset> {
        let portfolio = Self::get_user_portfolio(env.clone(), user);
        let mut assets = Vec::new(&env);
        
        for asset_id in portfolio.owned_assets.iter() {
            // Fixed dereferencing
            let asset = Self::get_asset(env.clone(), asset_id);
            assets.push_back(asset);
        }
        
        assets
    }

    // Get user's active leases
    pub fn get_user_active_leases(env: Env, user: Address) -> Vec<Lease> {
        let portfolio = Self::get_user_portfolio(env.clone(), user);
        let mut leases = Vec::new(&env);
        
        // Get leases as lessee - Fixed dereferencing
        for lease_id in portfolio.active_leases_as_lessee.iter() {
            let lease = Self::get_lease(env.clone(), lease_id);
            leases.push_back(lease);
        }
        
        // Get leases as lessor - Fixed dereferencing
        for lease_id in portfolio.active_leases_as_lessor.iter() {
            let lease = Self::get_lease(env.clone(), lease_id);
            if !leases.iter().any(|l| l.lease_id == lease.lease_id) {
                leases.push_back(lease);
            }
        }
        
        leases
    }
}