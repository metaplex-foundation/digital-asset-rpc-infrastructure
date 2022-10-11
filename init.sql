CREATE TABLE raw_txn
(
    signature varchar(64) PRIMARY KEY,
    slot      bigint not null,
    processed bool   not null
);

CREATE INDEX raw_slot on raw_txn (slot);

CREATE TABLE cl_items
(
    id       bigserial PRIMARY KEY,
    tree     BYTEA  NOT NULL,
    node_idx BIGINT NOT NULL,
    leaf_idx BIGINT,
    seq      BIGINT NOT NULL,
    level    BIGINT NOT NULL,
    hash     BYTEA  NOT NULL
);
-- Index All the things space is cheap
CREATE INDEX cl_items_tree_idx on cl_items (tree);
CREATE INDEX cl_items_hash_idx on cl_items (hash);
CREATE INDEX cl_items_level on cl_items (level);
CREATE INDEX cl_items_node_idx on cl_items (node_idx);
CREATE INDEX cl_items_leaf_idx on cl_items (leaf_idx);
CREATE UNIQUE INDEX cl_items__tree_node on cl_items (tree, node_idx);

CREATE TABLE backfill_items
(
    id         bigserial PRIMARY KEY,
    tree       BYTEA  NOT NULL,
    seq        BIGINT NOT NULL,
    slot       BIGINT NOT NULL,
    force_chk  bool,
    backfilled bool
);

CREATE INDEX backfill_items_tree_idx on backfill_items (tree);
CREATE INDEX backfill_items_seq_idx on backfill_items (seq);
CREATE INDEX backfill_items_slot_idx on backfill_items (slot);
CREATE INDEX backfill_items_force_chk_idx on backfill_items (force_chk);
CREATE INDEX backfill_items_backfilled_idx on backfill_items (backfilled);
CREATE INDEX backfill_items_tree_seq_idx on backfill_items (tree, seq);
CREATE INDEX backfill_items_tree_slot_idx on backfill_items (tree, slot);
CREATE INDEX backfill_items_tree_force_chk_idx on backfill_items (tree, force_chk);

CREATE
    or REPLACE FUNCTION notify_new_backfill_item()
    RETURNS trigger
    LANGUAGE 'plpgsql'
as
$BODY$
declare
begin
    if
        (tg_op = 'INSERT') then
        perform pg_notify('backfill_item_added', 'hello');
    end if;

    return null;
end
$BODY$;

CREATE TRIGGER after_insert_item
    AFTER INSERT
    ON backfill_items
    FOR EACH ROW
EXECUTE PROCEDURE notify_new_backfill_item();


-- START NFT METADATA
CREATE TYPE owner_type AS ENUM ('unknown', 'token', 'single');
CREATE TYPE royalty_target_type AS ENUM ('unknown', 'creators', 'fanout', 'single');
CREATE TYPE chain_mutability AS ENUM ('unknown', 'mutable', 'immutable');
CREATE TYPE mutability AS ENUM ('unknown', 'mutable', 'immutable');
CREATE TYPE v1_account_attachments AS ENUM ('unknown', 'edition', 'master_edition_v2', 'master_edition_v1', 'edition_marker');
CREATE TYPE specification_versions AS ENUM ('unknown', 'v0', 'v1', 'v2');
CREATE TYPE specification_asset_class AS ENUM ('unknown', 'FUNGIBLE_TOKEN', 'FUNGIBLE_ASSET', 'NFT', 'PRINTABLE_NFT', 'PRINT', 'TRANSFER_RESTRICTED_NFT', 'NON_TRANSFERABLE_NFT', 'IDENTITY_NFT');

create table tokens
(
    mint             bytea PRIMARY KEY,
    supply           bigint not null default 0,
    decimals         int    not null default 0,
    token_program    bytea  not null,
    mint_authority   bytea,
    freeze_authority bytea,
    close_authority  bytea,
    extension_data   bytea,
    slot_updated     bigint not null
);
create index t_mint_auth on tokens (mint_authority);
create index t_freeze_auth on tokens (freeze_authority);
create index t_close_auth on tokens (close_authority);
create index t_slot_updated_idx on tokens USING BTREE (slot_updated);
create index t_supply on tokens USING BTREE (supply);
create index t_decimals on tokens USING BTREE (decimals);

create table token_accounts
(
    pubkey           bytea PRIMARY KEY,
    mint             bytea references tokens (mint),
    amount           bigint not null default 0,
    owner            bytea  not null,
    frozen           bool   not null default false,
    close_authority  bytea,
    delegate         bytea,
    delegated_amount bigint not null default 0,
    slot_updated     bigint not null,
    token_program    bytea  not null
);
create index ta_delegate on token_accounts (delegate);
create index ta_slot_updated_idx on token_accounts USING BTREE (slot_updated);
create index ta_amount on token_accounts USING BTREE (amount);
create index ta_amount_del on token_accounts USING BTREE (delegated_amount);

create table asset_data
(
    id                    bytea PRIMARY KEY,
    chain_data_mutability chain_mutability not null default 'mutable',
    chain_data            jsonb            not null,
    metadata_url          varchar(200)     not null,
    metadata_mutability   mutability       not null default 'mutable',
    metadata              jsonb            not null,
    slot_updated          bigint           not null
);

create index slot_updated_idx on asset_data USING BTREE (slot_updated);

create table asset
(
    id                        bytea PRIMARY KEY,
    alt_id                    bytea,
-- Specification version determines alot of how this poly morphic table is handled
-- Specification is the MAJOR metaplex version, currently only v1
    specification_version     specification_versions    not null,
    specification_asset_class specification_asset_class not null,

    owner                     bytea,
    owner_type                owner_type                not null default 'single',
    -- delegation
    delegate                  bytea,
    -- freeze
    frozen                    bool                      not null default false,
    -- supply
    supply                    bigint                    not null default 1,
    supply_mint               bytea,
    -- compression
    compressed                bool                      not null default false,
    compressible              bool                      not null default false,
    seq                       bigint                    not null,
    -- -- Can this asset be compressed
    tree_id                   bytea,
    leaf                      bytea,
    nonce                     bigint                    not null,
    -- royalty
    royalty_target_type       royalty_target_type       not null default 'creators',
    royalty_target            bytea,
    royalty_amount            int                       not null default 0,
    -- data
    asset_data                bytea references asset_data (id),
    -- visibility
    created_at                timestamp with time zone           default (now() at time zone 'utc'),
    burnt                     bool                      not null default false,
    slot_updated              bigint                    not null
);

create index asset_tree on asset (tree_id);
create index asset_leaf on asset (leaf);
create index asset_tree_leaf on asset (tree_id, leaf);
create index asset_revision on asset (tree_id, leaf, nonce);
create index asset_owner on asset (owner);
create index asset_delegate on asset (delegate);

create table asset_v1_account_attachments
(
    id              bytea PRIMARY KEY,
    asset_id        bytea references asset (id),
    attachment_type v1_account_attachments not null,
    initialized     bool                   not null default false,
    data            jsonb,
    slot_updated    bigint                 not null
);

-- grouping
create table asset_grouping
(
    id           bigserial PRIMARY KEY,
    asset_id     bytea references asset (id) not null,
    group_key    text                        not null,
    group_value  text                        not null,
    seq          bigint                      not null,
    slot_updated bigint                      not null
);
-- Limit indexable grouping keys, meaning only create on specific keys, but index the ones we allow
create unique index asset_grouping_asset_id on asset_grouping (asset_id);
create index asset_grouping_key on asset_grouping (group_key, group_value);
create index asset_grouping_value on asset_grouping (group_key, asset_id);

-- authority
create table asset_authority
(
    id           bigserial PRIMARY KEY,
    asset_id     bytea references asset (id) not null,
    scopes       text[],
    authority    bytea                       not null,
    seq          bigint                      not null,
    slot_updated bigint                      not null
);
create unique index asset_authority_asset_id on asset_authority (asset_id);
create index asset_authority_idx on asset_authority (asset_id, authority);

-- creators
create table asset_creators
(
    id           bigserial PRIMARY KEY,
    asset_id     bytea references asset (id) not null,
    creator      bytea                       not null,
    share        int                         not null default 0,
    verified     bool                        not null default false,
    seq          bigint                      not null,
    slot_updated bigint                      not null
);
create unique index asset_creators_asset_id on asset_creators (asset_id);
create index asset_creator on asset_creators (asset_id, creator);
create index asset_verified_creator on asset_creators (asset_id, verified);

create type whitelist_mint_mode AS ENUM ('burn_every_time', 'never_burn');
create type end_setting_type AS ENUM ('date', 'amount');

create table candy_machine
(
    id                       bytea               PRIMARY KEY,
    features                 int,
    authority                bytea               not null,
    mint_authority           bytea,
    wallet                   bytea               not null,
    token_mint               bytea,
    items_redeemed           int                 not null,
    candy_guard_pda          bytea,
    version                  int                 not null,
    collection_mint          bytea,                            
    allow_thaw               bool,                                    
    frozen_count             int,                                      
    mint_start               int,
    freeze_time              int,                                     
    freeze_fee               int,                       
    created_at               timestamp with time zone     default (now() at time zone 'utc'),      
    last_minted              timestamp with time zone     default (now() at time zone 'utc')         
);

create table candy_machine_data
(
    id                                   bigserial        PRIMARY KEY,
    uuid                                 varchar(6),
    price                                int,
    symbol                               varchar(10)      not null,
    seller_fee_basis_points              int              not null,
    max_supply                           int              not null,
    is_mutable                           bool             not null,
    retain_authority                     bool,
    go_live_date                         int,
    items_available                      int              not null,
    candy_machine_id                     bytea references candy_machine (id),
    whitelist_mode                       whitelist_mint_mode,                                      
    whitelist_mint                       bytea,                                                    
    whitelist_presale                    bool,                                                    
    whitelist_discount_price             int,
    config_line_settings_prefix_name     varchar(10),                             
    config_line_settings_name_length     int,                                     
    config_line_settings_prefix_uri      varchar(10),                            
    config_line_settings_uri_length      int,                                    
    config_line_settings_is_sequential   bool,
    gatekeeper_network                   bytea,                                    
    gatekeeper_expire_on_use             bool,  
    end_setting_number                   int,                                     
    end_setting_type                     end_setting_type,    
    hidden_settings_name                 varchar(50),                              
    hidden_settings_uri                  varchar(200),                             
    hidden_settings_hash                 bytea                                                                    
);
create unique index candy_machine_data_candy_machine_id on candy_machine_data (candy_machine_id);

create table candy_machine_creators
(
    id                    bigserial                                PRIMARY KEY,
    candy_machine_id      bytea references candy_machine (id)      not null,
    creator               bytea                                    not null,
    share                 int                                      not null default 0,
    verified              bool                                     not null default false
);
create unique index candy_machine_creators_candy_machine_id on candy_machine_creators (candy_machine_id);
create index candy_machine_creator on candy_machine_creators (candy_machine_id, creator);
create index candy_machine_verified_creator on candy_machine_creators (candy_machine_id, verified);

create table candy_guard
(   
    id                   bytea                                   PRIMARY KEY,
    base                 bytea                                   not null,
    bump                 int                                     not null,
    authority            bytea                                   not null
);

create table candy_guard_group
(
    id                               bigserial                              PRIMARY KEY,
    label                            varchar(50),
    candy_guard_id                   bytea references candy_guard (id)      not null,
    whitelist_mode                   whitelist_mint_mode,                                      
    whitelist_mint                   bytea,                                                    
    whitelist_presale                bool,                                                    
    whitelist_discount_price         int,
    bot_tax_lamports                 int,
    bot_tax_last_instruction         bool,
    live_date                        int,    
    third_party_signer_key           bytea,    
    nft_payment_destination          bytea,                        
    nft_payment_required_collection  bytea,        
    allow_list_merkle_root           bytea,  
    mint_limit_id                    int,    
    mint_limit_limit                 int,     
    gatekeeper_network               bytea,                                    
    gatekeeper_expire_on_use         bool                                                                                  
);
create unique index candy_guard_group_candy_guard_id on candy_guard_group (candy_guard_id);























