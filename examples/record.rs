extern crate kiwisdr;
use std::sync::Arc;
use kiwisdr::{kiwi::{AMTuning, AgcConfig, Station}, KiwiSdrBuilder, KiwiSdrSndClient};
use tokio::sync::Mutex;
use url::Url;
use hound::{WavWriter, Sample, WavSpec};
use byteorder::{BigEndian, LittleEndian, ByteOrder, ReadBytesExt};
use clap::{arg, command, Parser};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Name of the person to greet
    #[arg(short, long)]
    endpoint: String,
}

#[tokio::main]
async fn main() {
    // Set up the logger
    simple_logger::init_with_level(log::Level::Info).unwrap();

    let args = Args::parse();
    let endpoint = Url::parse(&args.endpoint).expect("Invalid URL");

    let mut wav_writer = WavWriter::create("output.wav", WavSpec {
        channels: 1,
        sample_rate: 12000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    }).unwrap();


    let mut kiwi = match KiwiSdrBuilder::new(endpoint.clone())
        .with_name("SDR 1")
        .build_snd()
        .await {
            Ok(kiwi) => kiwi,
            Err(e) => {
                log::error!("Error connecting to {}: {:?}", endpoint, e);
                return;
            }
        };    

    kiwi.wait_for_start().await.unwrap();

    // kiwi.configure_agc(AgcConfig::new(false, false, -20, 6, 1000, 70)).await.unwrap();


    kiwi.tune(Station::AM(AMTuning { 
        bandwidth: 10000, 
        freq: 5000.0
    })).await.expect("Unable to tune");

    // kiwi.configure_agc(AgcConfig::new(true, false, -20, 6, 1000, 0)).await.unwrap();
    kiwi.configure_agc(AgcConfig::new(false, false, 0, 0, 0, 10)).await.unwrap();
    kiwi.set_compression(false).await.unwrap();

    let kiwi = Arc::new(Mutex::new(kiwi));
    let wav_writer = Arc::new(Mutex::new(wav_writer));

    let thread_kiwi = kiwi.clone();
    let thread_writer = wav_writer.clone();
    tokio::spawn(async move {
        while let Ok(data) = thread_kiwi.lock().await.get_sound_data().await {
            let mut writer = thread_writer.lock().await;
            let mut writer = writer.get_i16_writer(data.len() as u32);
            for sample in data {
                writer.write_sample(sample);
            }
            writer.flush().unwrap();
        }
    });

    {
        let mut kiwi = kiwi.lock().await;
        log::info!("Updating parameters");
        kiwi.set_callsign("KE8TXG").await.expect("Unable to set callsign");
        kiwi.set_location("Earth").await.expect("Unable to set location");
    }

    let kiwi_clone = kiwi.clone();
    tokio::spawn(async {
        let kiwi = kiwi_clone;
        loop {
           {
                let mut kiwi = kiwi.lock().await;
                kiwi.send_keepalive().await.unwrap();
                log::info!("RSSI: {}", kiwi.get_stats().await.unwrap().rssi);
           }

            tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

        }
    });


    log::info!("Recording... Press Ctrl-C to stop");
    tokio::signal::ctrl_c().await.unwrap();
    wav_writer.lock().await.flush().unwrap();
    log::info!("Done");
    
    
}