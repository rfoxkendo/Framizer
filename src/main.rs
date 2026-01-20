use std::fs::File;
use std::io::{BufReader, Read, ErrorKind};

use rust_ringitem_format::RingItem;   // We'll invent our own type.

// Ring item types for frames.... complete just so we sort of reserve
// these types.

const TRACE_FRAME_ITEM_TYPE : u32 = 50;
//const TDC_FRAME_ITEM_TYPE   : u32 = 51;   // comment so we don't warn.

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

#[derive(Debug)]
struct Frame {
    frame_start: u64,        // Coarse timestamp - frame start.
    data_size: u32,          // data size samples in the frame.
    data_offset: u16,        // where in the frame the samples start.
    data: Vec<u16>,          // data_size samples.
}

impl Frame {
    pub fn new(start : u64) -> Frame {
        Frame {
            frame_start : start,
            data_size   : 0,        // Must be computed
            data_offset : 0,        // Must be computed.
            data : Vec::new(),
        }
    }
}

// Data format in the file from Aaron:
#[derive(Debug)]
struct Trace {
    timestamp: u64,          // Coarse timestamp of the trace.
    data: Vec<u16>,         // data samples for the trace.
}

const FRAME_LENGTH: u64  = 512;   // Ticks in a window.
fn main() {
    let file = File::open("traces.dat").unwrap();
    let mut ring_file = File::create("frames.evt").unwrap();

    let mut reader = BufReader::new(file);

    // Read the traces from file.


    let mut frame_timestamp = 0;
    while let Some(trace) = read_next_trace(&mut reader) {

        // emit 0 length frames unitil the trace starts inside of it:
        // If the timetamp is already <frame_timestamp drop the trace:

        if trace.timestamp < frame_timestamp {
            println!("Dropping trace with timestamp 0x{:x} because it starts before the current frame timestamp 0x{:x}.", trace.timestamp, frame_timestamp);
            continue;
        }

        while trace.timestamp >= frame_timestamp + FRAME_LENGTH {

            // emit an empty frame.
            let empty_frame = Frame::new(frame_timestamp);

            write_ring_item(&mut ring_file, &empty_frame).expect("Failed to write empty frame");

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

            // Whole trace fits in the frame

            write_ring_item(&mut ring_file, &f).expect("Failed to write single frame trace");
            frame_timestamp += FRAME_LENGTH;
        
        } else {
            // We emit frames until the trace is consumed.  All but the first frame have offsets of 0.
            let mut cursor : usize = 0;    // where we are in the tracde.
            let mut first_frame = Frame::new(frame_timestamp);
            first_frame.data_offset = data_offset as u16;
            first_frame.data_size   = (FRAME_LENGTH - data_offset) as u32;   // this is what fits.
            first_frame.data.extend(&trace.data[0..first_frame.data_size as usize]);   // Extend the v ector with this slice.
            
            // emit first frame:

            write_ring_item(&mut ring_file, &first_frame).expect("Failed to write first frame of multi-frame trace");

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
                // Output an overflow frame:

                write_ring_item(&mut ring_file, &frame).expect("Failed to write overflow frame for multi-frame trace");

                cursor += frame.data_size as usize;
                frame_timestamp += FRAME_LENGTH;

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
//
// Write a frame as a ring item.
// The ring item will have:
// - A type of TRACE_FRAME_ITEM_TYPE
// - A body header with timestamp the frame start time.
//   source id and barrier type 0, since this is a test.
// - A ring item body that consists of:
//   frame.data_size
//   frame.data_offset
//   frame.data
fn write_ring_item(f : &mut File, frame: &Frame) -> std::io::Result<usize> {

    // Create and fill the ring item:
    let mut ring_item = RingItem::new_with_body_header(
        TRACE_FRAME_ITEM_TYPE, 
        frame.frame_start,
        0,0
    );
    ring_item.add(frame.data_size)  // Lead with the data size and 
        .add(frame.data_offset);    // frame offset.
    
    // In the loop below, word appears to be &u16 not u16 so it must
    // be dereferenced.  Determined this experimnentally.

    for word  in &frame.data {     // If the vector is empty this will add nothing.

        ring_item.add(*word);  
    }
    ring_item.write_item(f)
}