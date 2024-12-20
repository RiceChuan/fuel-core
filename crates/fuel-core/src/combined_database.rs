#[cfg(feature = "rocksdb")]
use crate::state::historical_rocksdb::StateRewindPolicy;
use crate::{
    database::{
        database_description::{
            gas_price::GasPriceDatabase,
            off_chain::OffChain,
            on_chain::OnChain,
            relayer::Relayer,
        },
        Database,
        GenesisDatabase,
        Result as DatabaseResult,
    },
    service::DbType,
};
#[cfg(feature = "test-helpers")]
use fuel_core_chain_config::{
    StateConfig,
    StateConfigBuilder,
};
#[cfg(feature = "test-helpers")]
use fuel_core_storage::tables::{
    Coins,
    ContractsAssets,
    ContractsLatestUtxo,
    ContractsRawCode,
    ContractsState,
    Messages,
};
use fuel_core_storage::Result as StorageResult;
use fuel_core_types::fuel_types::BlockHeight;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CombinedDatabaseConfig {
    pub database_path: PathBuf,
    pub database_type: DbType,
    pub max_database_cache_size: usize,
    #[cfg(feature = "rocksdb")]
    pub state_rewind_policy: StateRewindPolicy,
    #[cfg(feature = "rocksdb")]
    pub max_fds: i32,
}

/// A database that combines the on-chain, off-chain and relayer databases into one entity.
#[derive(Default, Clone)]
pub struct CombinedDatabase {
    on_chain: Database<OnChain>,
    off_chain: Database<OffChain>,
    relayer: Database<Relayer>,
    gas_price: Database<GasPriceDatabase>,
}

impl CombinedDatabase {
    pub fn new(
        on_chain: Database<OnChain>,
        off_chain: Database<OffChain>,
        relayer: Database<Relayer>,
        gas_price: Database<GasPriceDatabase>,
    ) -> Self {
        Self {
            on_chain,
            off_chain,
            relayer,
            gas_price,
        }
    }

    #[cfg(feature = "rocksdb")]
    pub fn prune(path: &std::path::Path) -> crate::database::Result<()> {
        crate::state::rocks_db::RocksDb::<OnChain>::prune(path)?;
        crate::state::rocks_db::RocksDb::<OffChain>::prune(path)?;
        crate::state::rocks_db::RocksDb::<Relayer>::prune(path)?;
        crate::state::rocks_db::RocksDb::<GasPriceDatabase>::prune(path)?;
        Ok(())
    }

    #[cfg(feature = "rocksdb")]
    pub fn open(
        path: &std::path::Path,
        capacity: usize,
        state_rewind_policy: StateRewindPolicy,
        max_fds: i32,
    ) -> crate::database::Result<Self> {
        // Split the fds in equitable manner between the databases
        let max_fds = match max_fds {
            -1 => -1,
            _ => max_fds.saturating_div(4),
        };
        // TODO: Use different cache sizes for different databases
        let on_chain =
            Database::open_rocksdb(path, capacity, state_rewind_policy, max_fds)?;
        let off_chain =
            Database::open_rocksdb(path, capacity, state_rewind_policy, max_fds)?;
        let relayer =
            Database::open_rocksdb(path, capacity, StateRewindPolicy::NoRewind, max_fds)?;
        let gas_price =
            Database::open_rocksdb(path, capacity, state_rewind_policy, max_fds)?;
        Ok(Self {
            on_chain,
            off_chain,
            relayer,
            gas_price,
        })
    }

    /// A test-only temporary rocksdb database with given rewind policy.
    #[cfg(feature = "rocksdb")]
    pub fn temp_database_with_state_rewind_policy(
        state_rewind_policy: StateRewindPolicy,
    ) -> DatabaseResult<Self> {
        Ok(Self {
            on_chain: Database::rocksdb_temp(state_rewind_policy)?,
            off_chain: Database::rocksdb_temp(state_rewind_policy)?,
            relayer: Default::default(),
            gas_price: Default::default(),
        })
    }

    pub fn from_config(config: &CombinedDatabaseConfig) -> DatabaseResult<Self> {
        let combined_database = match config.database_type {
            #[cfg(feature = "rocksdb")]
            DbType::RocksDb => {
                // use a default tmp rocksdb if no path is provided
                if config.database_path.as_os_str().is_empty() {
                    tracing::warn!(
                        "No RocksDB path configured, initializing database with a tmp directory"
                    );
                    CombinedDatabase::temp_database_with_state_rewind_policy(
                        config.state_rewind_policy,
                    )?
                } else {
                    tracing::info!(
                        "Opening database {:?} with cache size \"{}\" and state rewind policy \"{:?}\"",
                        config.database_path,
                        config.max_database_cache_size,
                        config.state_rewind_policy,
                    );
                    CombinedDatabase::open(
                        &config.database_path,
                        config.max_database_cache_size,
                        config.state_rewind_policy,
                        config.max_fds,
                    )?
                }
            }
            DbType::InMemory => CombinedDatabase::in_memory(),
            #[cfg(not(feature = "rocksdb"))]
            _ => CombinedDatabase::in_memory(),
        };

        Ok(combined_database)
    }

    pub fn in_memory() -> Self {
        Self::new(
            Database::in_memory(),
            Database::in_memory(),
            Database::in_memory(),
            Database::in_memory(),
        )
    }

    pub fn check_version(&self) -> StorageResult<()> {
        self.on_chain.check_version()?;
        self.off_chain.check_version()?;
        self.relayer.check_version()?;
        Ok(())
    }

    pub fn on_chain(&self) -> &Database<OnChain> {
        &self.on_chain
    }

    #[cfg(any(feature = "test-helpers", test))]
    pub fn on_chain_mut(&mut self) -> &mut Database<OnChain> {
        &mut self.on_chain
    }

    pub fn off_chain(&self) -> &Database<OffChain> {
        &self.off_chain
    }

    #[cfg(any(feature = "test-helpers", test))]
    pub fn off_chain_mut(&mut self) -> &mut Database<OffChain> {
        &mut self.off_chain
    }

    pub fn relayer(&self) -> &Database<Relayer> {
        &self.relayer
    }

    #[cfg(any(feature = "test-helpers", test))]
    pub fn relayer_mut(&mut self) -> &mut Database<Relayer> {
        &mut self.relayer
    }

    pub fn gas_price(&self) -> &Database<GasPriceDatabase> {
        &self.gas_price
    }

    #[cfg(any(feature = "test-helpers", test))]
    pub fn gas_price_mut(&mut self) -> &mut Database<GasPriceDatabase> {
        &mut self.gas_price
    }

    #[cfg(feature = "test-helpers")]
    pub fn read_state_config(&self) -> StorageResult<StateConfig> {
        use fuel_core_chain_config::AddTable;
        use fuel_core_producer::ports::BlockProducerDatabase;
        use fuel_core_storage::transactional::AtomicView;
        use fuel_core_types::fuel_vm::BlobData;
        use itertools::Itertools;
        let mut builder = StateConfigBuilder::default();

        macro_rules! add_tables {
            ($($table: ty),*) => {
                $(
                    let table = self
                        .on_chain()
                        .entries::<$table>(None, fuel_core_storage::iter::IterDirection::Forward)
                        .try_collect()?;
                    builder.add(table);
                )*
            };
        }

        add_tables!(
            Coins,
            Messages,
            BlobData,
            ContractsAssets,
            ContractsState,
            ContractsRawCode,
            ContractsLatestUtxo
        );

        let view = self.on_chain().latest_view()?;
        let latest_block = view.latest_block()?;
        let blocks_root =
            view.block_header_merkle_root(latest_block.header().height())?;
        let state_config =
            builder.build(Some(fuel_core_chain_config::LastBlockConfig::from_header(
                latest_block.header(),
                blocks_root,
            )))?;

        Ok(state_config)
    }

    /// Rollbacks the state of the blockchain to a specific block height.
    pub fn rollback_to<S>(
        &self,
        target_block_height: BlockHeight,
        shutdown_listener: &mut S,
    ) -> anyhow::Result<()>
    where
        S: ShutdownListener,
    {
        while !shutdown_listener.is_cancelled() {
            let on_chain_height = self
                .on_chain()
                .latest_height_from_metadata()?
                .ok_or(anyhow::anyhow!("on-chain database doesn't have height"))?;

            let off_chain_height = self
                .off_chain()
                .latest_height_from_metadata()?
                .ok_or(anyhow::anyhow!("off-chain database doesn't have height"))?;

            let gas_price_chain_height =
                self.gas_price().latest_height_from_metadata()?;

            let gas_price_rolled_back = gas_price_chain_height.is_none()
                || gas_price_chain_height.expect("We checked height before")
                    == target_block_height;

            if on_chain_height == target_block_height
                && off_chain_height == target_block_height
                && gas_price_rolled_back
            {
                break;
            }

            if on_chain_height < target_block_height {
                return Err(anyhow::anyhow!(
                    "on-chain database height({on_chain_height}) \
                    is less than target height({target_block_height})"
                ));
            }

            if off_chain_height < target_block_height {
                return Err(anyhow::anyhow!(
                    "off-chain database height({off_chain_height}) \
                    is less than target height({target_block_height})"
                ));
            }

            if let Some(gas_price_chain_height) = gas_price_chain_height {
                if gas_price_chain_height < target_block_height {
                    return Err(anyhow::anyhow!(
                        "gas-price-chain database height({gas_price_chain_height}) \
                        is less than target height({target_block_height})"
                    ));
                }
            }

            if on_chain_height > target_block_height {
                self.on_chain().rollback_last_block()?;
            }

            if off_chain_height > target_block_height {
                self.off_chain().rollback_last_block()?;
            }

            if let Some(gas_price_chain_height) = gas_price_chain_height {
                if gas_price_chain_height > target_block_height {
                    self.gas_price().rollback_last_block()?;
                }
            }
        }

        if shutdown_listener.is_cancelled() {
            return Err(anyhow::anyhow!(
                "Stop the rollback due to shutdown signal received"
            ));
        }

        Ok(())
    }

    /// This function is fundamentally different from `rollback_to` in that it
    /// will rollback the off-chain/gas-price databases if they are ahead of the
    /// on-chain database. If they don't have a height or are behind the on-chain
    /// we leave it to the caller to decide how to bring them up to date.
    /// We don't rollback the on-chain database as it is the source of truth.
    /// The target height of the rollback is the latest height of the on-chain database.
    pub fn sync_aux_db_heights<S>(&self, shutdown_listener: &mut S) -> anyhow::Result<()>
    where
        S: ShutdownListener,
    {
        while !shutdown_listener.is_cancelled() {
            let on_chain_height = match self.on_chain().latest_height_from_metadata()? {
                Some(height) => height,
                None => break, // Exit loop if on-chain height is None
            };

            let off_chain_height = self.off_chain().latest_height_from_metadata()?;
            let gas_price_height = self.gas_price().latest_height_from_metadata()?;

            // Handle off-chain rollback if necessary
            if let Some(off_height) = off_chain_height {
                if off_height > on_chain_height {
                    self.off_chain().rollback_last_block()?;
                }
            }

            // Handle gas price rollback if necessary
            if let Some(gas_height) = gas_price_height {
                if gas_height > on_chain_height {
                    self.gas_price().rollback_last_block()?;
                }
            }

            // If both off-chain and gas price heights are synced, break
            if off_chain_height.map_or(true, |h| h <= on_chain_height)
                && gas_price_height.map_or(true, |h| h <= on_chain_height)
            {
                break;
            }
        }

        Ok(())
    }
}

/// A trait for listening to shutdown signals.
pub trait ShutdownListener {
    /// Returns true if the shutdown signal has been received.
    fn is_cancelled(&self) -> bool;
}

/// A genesis database that combines the on-chain, off-chain and relayer
/// genesis databases into one entity.
#[derive(Default, Clone)]
pub struct CombinedGenesisDatabase {
    pub on_chain: GenesisDatabase<OnChain>,
    pub off_chain: GenesisDatabase<OffChain>,
}

impl CombinedGenesisDatabase {
    pub fn on_chain(&self) -> &GenesisDatabase<OnChain> {
        &self.on_chain
    }

    pub fn off_chain(&self) -> &GenesisDatabase<OffChain> {
        &self.off_chain
    }
}
