use clap::Parser;
use ordinals::{Etching, Rune};
use std::str::FromStr;
use bitcoin::{Network, Address, Amount};

/// CLI tools for funkybit arch tools.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct CliArgs {
    /// Address to premine runes to
    #[arg(short, long)]
    mint_address: String,

    /// Number of runes to premine
    #[arg(short, long, default_value_t = 1000000000000)]
    amount: u128,

    /// Number of runes to premine
    #[arg(long, default_value_t = 6)]
    divisibility: u8,

    /// name of the rune
    #[arg(long)]
    name: String,

    #[arg(long)]
    spacers: Option<u32>,

    /// name of the rune
    #[arg(long, default_value = "¢")]
    symbol: String,

    #[arg(short, long)]
    funding_tx_hex: String,

    #[arg(short, long, default_value_t = 2546)]
    postage: u64,

    #[arg(short, long, default_value_t = 2000)]
    etching_network_fee: u64,
}

use testutils::runes::build_commit_and_etch_transactions;


fn main() {
    let args = CliArgs::parse();

    let rune = Rune::from_str(&args.name).unwrap();

    let mint_address = Address::from_str(&args.mint_address)
        .unwrap()
        .require_network(Network::Regtest)
        .unwrap();

    let (commit_tx_hex, etching_tx_hex) = build_commit_and_etch_transactions(
        Etching {
            divisibility: Some(args.divisibility),
            premine: Some(args.amount),
            rune: Some(rune),
            spacers: args.spacers,
            symbol: Some(args.symbol.chars().nth(0).unwrap()),
            terms: None,
            turbo: false,
        },
        mint_address.clone(),
        args.funding_tx_hex,
        Amount::from_sat(args.postage),
        Amount::from_sat(args.etching_network_fee)
    );

    println!("{}:{}", commit_tx_hex, etching_tx_hex)
}
