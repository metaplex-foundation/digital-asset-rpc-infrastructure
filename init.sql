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
    id          bigserial PRIMARY KEY,
    tree        BYTEA  NOT NULL,
    seq         BIGINT NOT NULL,
    slot        BIGINT NOT NULL,
    force_chk   bool,
    backfilled  bool
);

CREATE INDEX backfill_items_tree_idx on backfill_items (tree);
CREATE INDEX backfill_items_seq_idx on backfill_items (seq);
CREATE INDEX backfill_items_slot_idx on backfill_items (slot);
CREATE INDEX backfill_items_force_chk_idx on backfill_items (force_chk);
CREATE INDEX backfill_items_backfilled_idx on backfill_items (backfilled);
CREATE INDEX backfill_items_tree_seq_idx on backfill_items (tree, seq);
CREATE INDEX backfill_items_tree_slot_idx on backfill_items (tree, slot);
CREATE INDEX backfill_items_tree_force_chk_idx on backfill_items (tree, force_chk);

CREATE or REPLACE FUNCTION notify_new_backfill_item()
    RETURNS trigger
     LANGUAGE 'plpgsql'
as $BODY$
declare
begin
    if (tg_op = 'INSERT') then
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

create table asset_data
(
    id                    bigserial PRIMARY KEY,
    chain_data_mutability chain_mutability not null default 'mutable',
    schema_version        int              not null default 1,
    chain_data            jsonb            not null,
    metadata_url          varchar(200)     not null,
    metadata_mutability   mutability       not null default 'mutable',
    metadata              jsonb            not null
);

create table asset
(
    id                    bytea PRIMARY KEY,
    specification_version int                 not null default 1,
    owner                 bytea               not null,
    owner_type            owner_type          not null default 'single',
    -- delegation
    delegate              bytea,
    -- freeze
    frozen                bool                not null default false,
    -- supply
    supply                bigint              not null default 1,
    supply_mint           bytea,
    -- compression
    compressed            bool                not null default false,
    seq                   bigint              not null,
    -- -- Can this asset be compressed
    compressible          bool                not null default false,
    tree_id               bytea,
    leaf                  bytea,
    nonce                 bigint              not null,
    -- royalty
    royalty_target_type   royalty_target_type not null default 'creators',
    royalty_target        bytea,
    royalty_amount        int                 not null default 0,
    -- data
    chain_data_id         bigint references asset_data (id),
    -- visibility
    created_at            timestamp with time zone     default (now() at time zone 'utc'),
    burnt                 bool                not null default false
);

create index asset_tree on asset (tree_id);
create index asset_leaf on asset (leaf);
create index asset_tree_leaf on asset (tree_id, leaf);
create index asset_revision on asset (tree_id, leaf, nonce);
create index asset_owner on asset (owner);
create index asset_delegate on asset (delegate);

-- grouping
create table asset_grouping
(
    id          bigserial PRIMARY KEY,
    asset_id    bytea references asset (id) not null,
    group_key   text                        not null,
    group_value text                        not null,
    seq         bigint                      not null
);
-- Limit indexable grouping keys, meaning only create on specific keys, but index the ones we allow
create unique index asset_grouping_asset_id on asset_grouping (asset_id);
create index asset_grouping_key on asset_grouping (group_key, group_value);
create index asset_grouping_value on asset_grouping (group_key, asset_id);

-- authority
create table asset_authority
(
    id        bigserial PRIMARY KEY,
    asset_id  bytea references asset (id) not null,
    scopes    text[],
    authority bytea                       not null,
    seq       bigint                      not null
);
create unique index asset_authority_asset_id on asset_authority (asset_id);
create index asset_authority_idx on asset_authority (asset_id, authority);

-- creators
create table asset_creators
(
    id       bigserial PRIMARY KEY,
    asset_id bytea references asset (id) not null,
    creator  bytea                       not null,
    share    int                         not null default 0,
    verified bool                        not null default false,
    seq      bigint                      not null
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
);

create table candy_machine_data
(
    id                         bigserial        PRIMARY KEY,
    uuid                       varchar(6),
    price                      int,
    symbol                     varchar(5)       not null,
    seller_fee_basis_points    int              not null,
    max_supply                 int              not null,
    is_mutable                 bool             not null,
    retain_authority           bool,
    go_live_date               int,
    items_available            int              not null,
    candy_machine_id           bytea references candy_machine (id),
    mode                       whitelist_mint_mode,                                      
    whitelist_mint             bytea,                                                    
    presale                    bool,                                                    
    discount_price             int,
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
    bump                 int                                     not null,
    authority            bytea                                   not null,
)

create table candy_guard_group
(
    id                   bigserial                              PRIMARY KEY,
    label                varchar(50)                            not null,
    candy_guard_id       bytea references candy_guard (id)      not null,
    mode                 whitelist_mint_mode,                                      
    whitelist_mint       bytea,                                                    
    presale              bool,                                                    
    discount_price       int,
)

-- TODO should version be an enum on cm table
create table candy_machine_hidden_settings
(
    id                    bigserial                                PRIMARY KEY,
    candy_machine_id      bytea references candy_machine (id),
    candy_guard_group     int references candy_guard_group (id),
    name                  varchar(50)                              not null,
    uri                   varchar(200)                             not null,
    hash                  bytea                                    not null
) 
create unique index candy_machine_hidden_settings_candy_machine_id on candy_machine_hidden_settings (candy_machine_id);
create unique index candy_machine_hidden_settings_candy_guard_group on candy_machine_hidden_settings (candy_guard_group);


create table candy_machine_end_settings
(
    id                    bigserial                                PRIMARY KEY,
    candy_machine_id      bytea references candy_machine (id),
    candy_guard_group     int references candy_guard_group (id),
    number                int                                      not null,
    end_setting_type      end_setting_type                         not null
    
) 
create unique index candy_machine_end_settings_candy_machine_id on candy_machine_end_settings (candy_machine_id);
create unique index candy_machine_end_settings_candy_guard_group on candy_machine_whitelist_mint_settings (candy_guard_group);

create table candy_machine_gatekeeper
(
    id                    bigserial                                PRIMARY KEY,
    candy_machine_id      bytea references candy_machine (id),
    candy_guard_group     int references candy_guard_group (id),
    gatekeeper_network    bytea                                    not null,
    expire_on_use         bool                                     not null
    
) 
create unique index candy_machine_gatekeeper_candy_machine_id on candy_machine_gatekeeper (candy_machine_id);
create unique index candy_machine_gatekeeper_candy_guard_group on candy_machine_gatekeeper (candy_guard_group);

create table candy_machine_config_line_settings
(
    id                    bigserial                               PRIMARY KEY,
    candy_machine_id      bytea references candy_machine (id)     not null,
    prefix_name           varchar(10)                             not null,
    name_length           int                                     not null,
    prefix_uri            varchar(10)                             not null,
    uri_length            int                                     not null,
    is_sequential         bool                                    not null,
)
create unique index candy_machine_config_line_settings_candy_machine_id on candy_machine_config_line_settings (candy_machine_id);

create table candy_guard_mint_limit
(
    id                   bigserial                               PRIMARY KEY,
    limit                int                                     not null,
    candy_guard_group    int references candy_guard_group (id),
)
create unique index candy_guard_mint_limit_candy_guard_group on candy_guard_mint_limit (candy_guard_group);


create table candy_guard_allow_list
(
    id                   bigserial                              PRIMARY KEY,
    merkle_root          bytea                                  not null,
    candy_guard_group    int references candy_guard_group (id), 
)
create unique index candy_guard_allow_list_candy_guard_group on candy_guard_allow_list (candy_guard_group);

create table candy_guard_nft_payment
(
    id                   bigserial                              PRIMARY KEY,
    burn                 bool                                   not null,
    required_collection  bytea                                  not null,
    candy_guard_group    int references candy_guard_group (id),
)
create unique index candy_guard_nft_payment_candy_guard_group on candy_guard_nft_payment (candy_guard_group);

create table candy_guard_third_party_signer
(
    id                   bigserial                              PRIMARY KEY,
    signer_key           bytea                                  not null,
    candy_guard_group    int references candy_guard_group (id),
)
create unique index candy_guard_third_party_signer_candy_guard_group on candy_guard_third_party_signer (candy_guard_group);

create table candy_guard_live_date
(
    id                   bigserial                              PRIMARY KEY,
    date                 int,
    candy_guard_group    int references candy_guard_group (id),
)

create table candy_guard_spl_token
(
    id                   bigserial                              PRIMARY KEY,
    amount               int                                    not null,
    token_mint           bytea                                  not null,
    destination_ata      bytea                                  not null,
    candy_guard_group    int references candy_guard_group (id),
)
create unique index candy_guard_spl_token_candy_guard_group on candy_guard_spl_token (candy_guard_group);

create table candy_guard_lamports
(
    id                   bigserial                              PRIMARY KEY,
    amount               int                                    not null,
    destination          bytea                                  not null,
    candy_guard_group    int references candy_guard_group (id),
)
create unique index candy_guard_lamports_candy_guard_group on candy_guard_lamports (candy_guard_group);

create table candy_guard_bot_tax
(
    id                   bigserial                              PRIMARY KEY,
    lamports             int                                    not null,
    last_instruction     bool                                   not null,
    candy_guard_group    int references candy_guard_group (id),
)
create unique index candy_guard_bot_tax_candy_guard_group on candy_guard_bot_tax (candy_guard_group);



