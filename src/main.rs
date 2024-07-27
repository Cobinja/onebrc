use core::str;
use std::{collections::BTreeMap, fmt::{Debug, Display}, fs::{metadata, File}, io::{Read, Seek, SeekFrom}, sync::{Arc, Mutex}, thread};
struct Station {
    min: f64,
    max: f64,
    sum: f64,
    values_read: u64,
}

impl Default for Station {
    fn default() -> Self {
        Self {
            min: f64::MAX,
            max: f64::MIN,
            sum: 0.0,
            values_read: 0
        }
    }
}

impl Station {
    fn update(&mut self, value: f64) {
        self.min = self.min.min(value);
        self.max = self.max.max(value);
        self.sum += value;
        self.values_read += 1;
    }
    
    fn merge(&mut self, other: Station) {
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
        self.sum += other.sum;
        self.values_read += other.values_read;
    }
}

impl Debug for Station {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Station").field("min", &self.min).field("max", &self.max).field("sum", &self.sum).field("values_read", &self.values_read).finish()
    }
}

impl Display for Station {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.1}/{:.1}/{:.1}", self.min, self.sum / self.values_read as f64, self.max)
    }
}

fn main() {
    let filename = match std::env::args().skip(1).next() {
        Some(name) => name,
        None => "../1brc/data/weather_stations.csv".to_owned(),
    };
    
    let length: usize = metadata(filename.clone()).expect("Unable to query file details").len().try_into().expect("Couldn't convert len from u64 to usize");
    let cores: usize = std::thread::available_parallelism().unwrap().get();
    // How much each thread should read
    let division: usize = ((length / cores) as f64).ceil() as usize;
    let mut starting_offsets = vec![0 as usize];
    
    let mut file = File::open(filename.clone()).expect("Unable to open file");
    
    // find newline ending for each thread
    for _i in 0..cores {
        file.seek(SeekFrom::Current(division as i64)).expect("Couldn't seek to position in file");
        if (file.stream_position().unwrap() as usize) >= length {
            break;
        }
        let mut buf = vec![0];
        loop {
            let _ = file.read(&mut buf);
            if buf[0] == b'\n' || buf[0] == 0 {
                break;
            }
        }
        starting_offsets.push(file.stream_position().unwrap() as usize);
    }
    
    let results = Arc::new(Mutex::new(BTreeMap::<String, Station>::new()));
    
    // Use scoped threads to keep things simpler
    thread::scope(|scope| {
        for i in 0..starting_offsets.len() {
            let filename = filename.clone();
            let starting_offsets = starting_offsets.clone();
            let results = results.clone();
            scope.spawn(move || {
                // read chunk with the size defined by starting_offsets
                let mut thread_file = File::open(&filename).expect("Unable to open file");
                
                let offset: usize = starting_offsets[i];
                let size = match i < starting_offsets.len() - 1 {
                    true => starting_offsets[i + 1] - starting_offsets[i],
                    false => length - starting_offsets[i],
                };
                let mut contents: Vec<u8> = vec![0_u8; size];
                thread_file.seek(SeekFrom::Start(offset as u64)).expect("Couldn't seek to position in file");
                thread_file.read(&mut contents).expect("Couldn't read file");
                
                // process data
                if *(contents.last().unwrap()) == b'\n' {
                    contents.pop();
                }
                let mut block_results = BTreeMap::<&[u8], Station>::new();
                let content_len = contents.len();
                let mut i: usize = 0;
                let mut last_idx: usize = 0;
                while i < content_len {
                    if contents[i] == b'\n' {
                        // process line data
                        let mut semicolon_idx: usize = 0;
                        for i in last_idx..i {
                            if contents[i] == b';' {
                                semicolon_idx = i;
                            }
                        }
                        
                        let name = &contents[last_idx..semicolon_idx];
                        let temp = str::from_utf8(&contents[semicolon_idx + 1..i]).unwrap().parse::<f64>().unwrap();
                        block_results.entry(name).or_default().update(temp);
                        
                        last_idx = i + 1;
                        i +=1;
                        continue;
                    }
                    i += 1;
                }
                if last_idx < content_len - 1 {
                    // process last line data
                    let mut semicolon_idx: usize = 0;
                    for i in last_idx..i {
                        if contents[i] == b';' {
                            semicolon_idx = i;
                        }
                    }
                    let name = &contents[last_idx..semicolon_idx];
                    let temp = str::from_utf8(&contents[semicolon_idx + 1..i]).unwrap().parse::<f64>().unwrap();
                    
                    block_results.entry(name).or_default().update(temp);
                }
                
                let mut results_writer = results.lock().expect("Could not lock");
                for (name, station) in block_results {
                    
                    results_writer.entry(str::from_utf8(name).unwrap().to_string()).or_default().merge(station);
                }
                drop(results_writer);
            });
        }
    });
    
    // print results
    print!("{{");
    let result_map = results.lock().unwrap();
    let mut iter = result_map.iter().take(result_map.len() - 1).peekable();
    while iter.peek().is_some() {
        let (name_val, station) = iter.next().unwrap();
        print!("{}={}, ", *name_val, station);
    }
    
    let (name_val, station) = result_map.last_key_value().unwrap();
    print!("{}={}", *name_val, station);
    println!("}}");
}
