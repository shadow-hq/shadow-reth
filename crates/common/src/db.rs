use std::str::FromStr;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Pool, Sqlite,
};

/// Utility function for constructing a pool and creating the necessary
/// tables and indices for storing shadow logs.
pub async fn setup_sqlite_db(db_path: &str) -> Result<Pool<Sqlite>, sqlx::Error> {
    let pool = create_pool(db_path).await?;
    create_tables(&pool).await?;
    create_indices(&pool).await?;
    Ok(pool)
}

async fn create_pool(db_path: &str) -> Result<Pool<Sqlite>, sqlx::Error> {
    SqlitePoolOptions::new()
        .connect_with(SqliteConnectOptions::from_str(db_path)?.create_if_missing(true))
        .await
}

// TODO: Create helper functions for inserting and retrieving data into shadow_logs table
// since SQLite has some weird restrictions when working with hexadecimal data

async fn create_tables(pool: &Pool<Sqlite>) -> Result<(), sqlx::Error> {
    let sql = r#"
        CREATE TABLE IF NOT EXISTS shadow_logs(
            block_number      	bigint  	not null,  
            block_hash        	varchar(66) not null,  
            block_timestamp   	bigint  	not null,  
            transaction_index 	bigint  	not null,  
            transaction_hash  	varchar(66) not null,  
            block_log_index   	bigint  	not null,  
            transaction_log_index bigint  	not null,  
            address           	varchar(42) not null,  
            data              	text,  
            topic_0           	varchar(66),  
            topic_1           	varchar(66),  
            topic_2           	varchar(66),  
            topic_3           	varchar(66),
            removed           	boolean,
            created_at        	datetime,  
            updated_at        	datetime
        )
        "#;

    let _ = sqlx::query(sql).execute(pool).await?;
    Ok(())
}

async fn create_indices(pool: &Pool<Sqlite>) -> Result<(), sqlx::Error> {
    let sql = r#"
        CREATE INDEX IF NOT EXISTS idx_shadow_logs_address ON shadow_logs (address);
        CREATE INDEX IF NOT EXISTS idx_shadow_logs_block_number ON shadow_logs (block_number);
        CREATE INDEX IF NOT EXISTS idx_shadow_logs_topic_0 ON shadow_logs (topic_0);
        CREATE INDEX IF NOT EXISTS idx_shadow_logs_topic_1 ON shadow_logs (topic_1);
        CREATE INDEX IF NOT EXISTS idx_shadow_logs_topic_2 ON shadow_logs (topic_2);
        CREATE INDEX IF NOT EXISTS idx_shadow_logs_topic_3 ON shadow_logs (topic_3);
        CREATE INDEX IF NOT EXISTS idx_shadow_logs_transaction_hash ON shadow_logs (transaction_hash);
        CREATE INDEX IF NOT EXISTS idx_shadow_logs_removed ON shadow_logs (removed);
        "#;

    let _ = sqlx::query(sql).execute(pool).await?;
    Ok(())
}

/// Seed test data.
pub async fn seed_test_data(pool: &Pool<Sqlite>) -> Result<(), sqlx::Error> {
    let sql_0 = r#"
        INSERT INTO shadow_logs (
            block_number,
            block_hash,
            block_timestamp,
            transaction_index,
            transaction_hash,
            block_log_index,
            transaction_log_index,
            address,
            data,
            topic_0,
            topic_1,
            topic_2,
            topic_3,
            removed,
            created_at,
            updated_at
        ) VALUES (
            18870000,
            X'4131d538cf705c267da7f448ec7460b177f40d28115ad290ba6a1fd734afe280',
            1703595263,
            167,
            X'8bf2361656e0ea6f338ad17ac3cd616f8eea9bb17e1afa1580802e9d3231c203',
            0,
            26,
            X'0fbc0a9be1e87391ed2c7d2bb275bec02f53241f',
            X'000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000049dc9ce34ad2a2177480000000000000000000000000000000000000000000000000432f754f7158ad80000000000000000000000000000000000000000000000000000000000000000',
            X'd78ad95fa46c994b6551d0da85fc275fe613ce37657fb8d5e3d130840159d822',
            X'0000000000000000000000003fc91a3afd70395cd496c647d5a6cc9d4b2b7fad',
            X'0000000000000000000000003fc91a3afd70395cd496c647d5a6cc9d4b2b7fad',
            null,
            false,
            date(),
            date()
        )
        "#;
    let sql_1 = r#"
        INSERT INTO shadow_logs (
            block_number,
            block_hash,
            block_timestamp,
            transaction_index,
            transaction_hash,
            block_log_index,
            transaction_log_index,
            address,
            data,
            topic_0,
            topic_1,
            topic_2,
            topic_3,
            removed,
            created_at,
            updated_at
        ) VALUES (
            18870001,
            X'3cac643a6a1af584681a6a6dc632cd110a479c9c642e2da92b73fefb45739165',
            1703595275,
            2,
            X'd02dc650cc9a34def3d7a78808a36a8cb2e292613c2989f4313155e8e4af9b0f',
            0,
            0,
            X'c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2',
            X'0000000000000000000000000000000000000000000000001bc16d674ec80000',
            X'e1fffcc4923d04b559f4d29a8bfc6cda04eb5b0d3c460751c2402c5c5cc9109c',
            X'0000000000000000000000003fc91a3afd70395cd496c647d5a6cc9d4b2b7fad',
            null,
            null,
            false,
            date(),
            date()
        )
        "#;

    let _ = sqlx::query(sql_0).execute(pool).await?;
    let _ = sqlx::query(sql_1).execute(pool).await?;
    Ok(())
}
