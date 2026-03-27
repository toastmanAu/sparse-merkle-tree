use crate::error::Error;
use alloc::vec::Vec;
use ckb_std::{env, syscalls::current_cycles};
use sparse_merkle_tree::{H256, SMTBuilder};

//
// format of arguments(all in hex format)
// <root hash> <proof> <key 1> <value 1> <key 2> <value 2> ... <key N> <value N>
// proof with variable length
// root hash, key and value are with fixed length: 32 bytes
//
pub(crate) fn entry() -> Result<(), Error> {
    let argv = env::argv();

    if argv.len() < 2 {
        return Err(Error::InvalidArgs);
    }

    if (argv.len() - 2) % 2 != 0 {
        return Err(Error::InvalidArgs);
    }

    let root_hash: H256 = {
        let bytes = hex::decode(argv[0].to_bytes()).map_err(|_| Error::InvalidArgs)?;
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        arr.into()
    };

    let proof: Vec<u8> = hex::decode(argv[1].to_bytes()).map_err(|_| Error::InvalidArgs)?;

    let mut smt_builder = SMTBuilder::new();

    let pair_count = (argv.len() - 2) / 2;

    for i in 0..pair_count {
        let key: H256 = {
            let bytes = hex::decode(argv[2 + 2 * i].to_bytes()).map_err(|_| Error::InvalidArgs)?;
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            arr.into()
        };
        let value: H256 = {
            let bytes =
                hex::decode(argv[2 + 2 * i + 1].to_bytes()).map_err(|_| Error::InvalidArgs)?;
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            arr.into()
        };
        smt_builder = smt_builder
            .insert(&key, &value)
            .map_err(|_| Error::VerifySmtFail)?;
    }

    #[cfg(feature = "enable_log")]
    log::info!(
        "Input summary => proof length: {} bytes, pairs count: {}",
        proof.len(),
        pair_count
    );

    let smt = smt_builder.build().map_err(|_| Error::VerifySmtFail)?;
    let last_cycles = current_cycles();
    smt.verify(&root_hash, &proof).map_err(|_| {
        #[cfg(feature = "enable_log")]
        log::info!("SMT verification failed.");
        Error::VerifySmtFail
    })?;
    log::info!("SMT verify() costs: {} K cycles", (current_cycles() - last_cycles)/1000);
    
    #[cfg(feature = "enable_log")]
    log::info!("SMT verification succeeded.");

    Ok(())
}
