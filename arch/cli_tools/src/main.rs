use clap::Parser;
use common::models::CallerInfo;
use ordinals::{Etching, Rune};
use testutils::ordclient::{OrdClient, wait_for_block};
use std::str::FromStr;
use bitcoin::{Network, Address};

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

}

use testutils::runes::etch_rune;


fn main() {
    let args = CliArgs::parse();

    let rune = Rune::from_str(&args.name).unwrap();
    let wallet = CallerInfo::generate_new().unwrap();

    let mint_address = Address::from_str(&args.mint_address)
        .unwrap()
        .require_network(Network::Regtest)
        .unwrap();

    let rune_id = etch_rune(
        &wallet,
        Etching {
            divisibility: Some(args.divisibility),
            premine: Some(args.amount),
            rune: Some(rune),
            spacers: args.spacers,
            symbol: Some(args.symbol.chars().nth(0).unwrap()),
            terms: None,
            turbo: false,
        },
        Some(mint_address.clone())
    );

    let ord_client = OrdClient::new("http://localhost:7080".to_string());
    wait_for_block(&ord_client, rune_id.block);
    let _ = ord_client.fetch_rune_details(rune_id);
    println!("{:?}", rune_id.to_string())
}
