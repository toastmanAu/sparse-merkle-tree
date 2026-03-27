#![cfg_attr(not(any(test)), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(test)]
extern crate alloc;

#[cfg(not(any(test)))]
ckb_std::entry!(program_entry);
#[cfg(not(any(test)))]
ckb_std::default_alloc!(16384, 1258306, 64);

mod entry;
mod error;

pub fn program_entry() -> i8 {
    #[cfg(feature = "enable_log")]
    {
        drop(ckb_std::logger::init());
        log::info!("smt-nostd-runner, log enabled");
    }
    match entry::entry() {
        Ok(_) => 0,
        Err(e) => {
            #[cfg(feature = "enable_log")]
            log::error!("error: {:?}", e);
            e.error_code()
        }
    }
}
