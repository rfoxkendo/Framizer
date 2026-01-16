use std::fs::File;
use std::io::{BufReader, Read, ErrorKind};

// In the end, we want to make a file that is 
// made up of ring items that are frames.
// a frame is a fixed size time chunk consists of the following:
// - The value 2 indicating this is a waveform frame.
// - a timestamp for the frame start.
// - a size of non-zero data in that frame.
// - offset (fine time) intot he fram of the data.
// size u16 data items representing the chunk of the
// trace that fit into the window.

// note that traces can span frame boundaries.
// Note that we're not going to support multiple trace starts in a window....
// if we see such a thing, we drop the second trace on the floor and output
// a message saying we did that.

// internally:

struct Frame {
    frame_type : u32,        // Always 2 (would be 1 for tdc data).
    frame_start: u64,        // Coarse timestamp - frame start.
    data_size: u32,          // data size samples in the frame.
    data_offset: u16,        // where in the frame the samples start.
    data: Vec<u16>,          // data_size samples.
}
impl Frame {
    pub fn new(start : u64) -> Frame {
        Frame {
            frame_type : 2,
            frame_start : start,
            data_size   : 0,        // Must be computed
            data_offset : 0,        // Must be computed.
            data : Vec::new(),
        }
    }
}

// Data format in the file from Aaron:

struct Trace {
    timestamp: u64,          // Coarse timestamp of the trace.
    data: Vec<u16>,         // data samples for the trace.
}

const FRAME_LENGTH: u64  = 512;   // Ticks in a window.
fn main() {
    let file = File::open("traces.dat").unwrap();
    let mut reader = BufReader::new(file);

    // Read the traces from file.


    let mut frame_timestamp = 0;
    let dummy_frame = Frame::new(frame_timestamp);
    println!("Frames will be of type {} ", dummy_frame.frame_type);
    while let Some(trace) = read_next_trace(&mut reader) {
        // emit 0 length frames unitil the trace starts inside of it:
        // If the timetamp is already <frame_timestamp drop the trace:

        if trace.timestamp < frame_timestamp {
            println!("Dropping trace with timestamp 0x{:x} because it starts before the current frame timestamp 0x{:x}.", trace.timestamp, frame_timestamp);
            continue;
        }
        println!("trace at timestamp 0x{:x}", trace.timestamp);        
        while trace.timestamp >= frame_timestamp + FRAME_LENGTH {

            // emit an empty frame.
            println!("Emitting empty frame with timestamp 0x{:x}.", frame_timestamp);
            frame_timestamp += FRAME_LENGTH;
        }
        // The trace starts in this frame:

        let  data_offset = trace.timestamp - frame_timestamp;
        
        if ((trace.data.len() + data_offset as usize) as u64) < FRAME_LENGTH {
            // Whole trace fits.

            let mut f = Frame::new(frame_timestamp);
            f.data_size = trace.data.len() as u32;                    // Whole trace fits.
            f.data_offset = data_offset as u16;
            f.data        = trace.data.clone();                     // whole trace.

            println!("Whole trace fits in frame frame ts 0x{:x}, offset {}, length {} ", f.frame_start, f.data_offset, f.data_size);
            frame_timestamp += FRAME_LENGTH;
        
        } else {
            // We emit frames until the trace is consumed.  All but the first frame have offsets of 0.
            let mut cursor : usize = 0;    // where we are in the tracde.
            let mut first_frame = Frame::new(frame_timestamp);
            first_frame.data_offset = data_offset as u16;
            first_frame.data_size   = (FRAME_LENGTH - data_offset) as u32;   // this is what fits.
            first_frame.data.extend(&trace.data[0..first_frame.data_size as usize]);   // Extend the v ector with this slice.
            println!("First frame ts 0x{:x} offset {}, length {}", first_frame.frame_start, first_frame.data_offset, first_frame.data_size);
            cursor += first_frame.data_size as usize;                           // next slice.
            frame_timestamp += FRAME_LENGTH;

            // Theoretically there could be multiple overflows.
            
            while cursor < trace.data.len() {
                let mut frame = Frame::new(frame_timestamp);
                frame.data_offset = 0;              // overflows into this frame.
                if trace.data.len() - cursor > FRAME_LENGTH as usize {
                    frame.data_size   = FRAME_LENGTH as u32;   // full filled
                    frame.data.extend(&trace.data[cursor .. cursor+FRAME_LENGTH as usize]);
                    
                } else {
                    frame.data_size = (trace.data.len() - cursor) as u32;
                    frame.data.extend(&trace.data[cursor..]);   // Rest of the trace.

                    
                    
                }
                cursor += frame.data_size as usize;
                frame_timestamp += FRAME_LENGTH;

                //output what we did:

                println!("Overflow frame with ts 0x{:x}  offset {}, length {} ", frame.frame_start, frame.data_offset, frame.data_size);
            }
        }
    }
    
    
}


// Read the next trace from the file:
// Returns None if we reach the end of the file.
fn read_next_trace(reader: &mut BufReader<File>) -> Option<Trace> {
    let mut timestamp_buf = [0u8; 8];
    let mut data_size_buf = [0u8; 4];   

    // Read the timestamp (8 bytes).
    if let Err(e) = reader.read_exact(&mut timestamp_buf) {
        if e.kind() == ErrorKind::UnexpectedEof {   
            return None; // End of file reached.
        } else {
            panic!("Error reading timestamp: {:?}", e);
        }
    }
    let timestamp = u64::from_le_bytes(timestamp_buf);

    // Read the data size (2 bytes).
    if let Err(e) = reader.read_exact(&mut data_size_buf) {
        if e.kind() == ErrorKind::UnexpectedEof {
            return None; // End of file reached.
        } else {
            panic!("Error reading data size: {:?}", e);
        }
    }
    let data_size = u32::from_le_bytes(data_size_buf);

    // Read the data samples (data_size * 2 bytes).

    let mut samples : Vec<u16> = Vec::with_capacity(data_size as usize);
    for _ in 0..data_size {
        let mut sample_buf = [0u8; 2];
        if let Err(e) = reader.read_exact(&mut sample_buf) {
            if e.kind() == ErrorKind::UnexpectedEof {
                return None; // End of file reached.
            } else {
                panic!("Error reading data sample: {:?}", e);
            }
        }
        let sample = u16::from_le_bytes(sample_buf);
        samples.push(sample);
    } 
    Some(Trace { timestamp: timestamp, 
        data: samples 
    })
}
