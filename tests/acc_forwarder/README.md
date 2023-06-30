# Account Forwarder

Account Forwarder is a tool designed to replay account changes, primarily originating from geyser. It forwards serialized account information to a specified Redis endpoint.

## Usage

### Send a single account

To forward information about a single account, run the following command:

`cargo run -- --redis-url <REDIS_URL> --rpc-url <RPC_URL> single --account <ACCOUNT>`

### Send mint, metadata, and owned token account

To forward mint, metadata, and owned token account information for a specific mint, use the following commands.

Locally:
`cargo run -- --redis-url redis://localhost:6379 --rpc-url $RPC_URL mint --mint t8nGUrFQozLtgiqnc5Pu8yiodbrJCaFyE3CGeubAvky`

Dev/Prod:
`cargo run -- --redis-url $REDIS_URL --rpc-url $RPC_URL mint --mint t8nGUrFQozLtgiqnc5Pu8yiodbrJCaFyE3CGeubAvky`

### Process accounts from a file

To forward account information for multiple accounts listed in a file, execute the following command:

`cargo run -- --redis-url <REDIS_URL> --rpc-url <RPC_URL> scenario --scenario-file <FILENAME>`

Replace <REDIS_URL>, <RPC_URL>, <ACCOUNT>, and <FILENAME> with the appropriate values for your use case.
