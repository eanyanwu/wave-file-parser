//! A library for parsing '.wav' files.
//! [This](http://www-mmsp.ece.mcgill.ca/Documents/AudioFormats/WAVE/Docs/riffmci.pdf) is the file specification that was followed.

// The `WaveFormatCategory`  enum has non-camel cased type names.
// This disables the default warning rust gives for such situtations
#![allow(non_camel_case_types)]

use std::fs;
use wave::WaveFileParser;

fn main() {
    // Example:

    // Get the first argument. We are assuming it will be a file to a .wav file.
    let args: Vec<String> = std::env::args().collect();
    let filename = &args[1];

    // Read the file as a sequence of bytes and feed said bytes into the parser
    // Get a WaveFile structure back.
    WaveFileParser::parse(fs::read(filename).unwrap());
}


mod wave {
    const BYTES_CHUNK_ID: usize = 4;
    const BYTES_CHUNK_SIZE: usize = 4;
    const BYTES_LIST_TYPE: usize = 4;

    // This library only supports samples up to 16 bits
    #[derive(Clone)]
    pub enum Sample {
        BitDepth8(u8),
        BitDepth16(i16),
    }

    // This library only supports wave files created using the Pulse Code Modulation format
    enum WaveFormatCategory {
        WAVE_FORMAT_PCM = 0x0001,
    }

    // The structure of the wave file that will be returned by the call to 
    // WaveFileParser::parse()
    pub struct WaveFile {
        pub channels: Vec<Vec<Sample>>,
        wave_format: WaveFormatCategory,
        pub sample_rate: u32,
        pub byte_rate: u32,
        pub block_align: u16,
        pub bits_per_sample: u16,
    }
 
    impl Default for WaveFile {
        fn default() -> Self {
            WaveFile {
                channels: vec![],
                wave_format: WaveFormatCategory::WAVE_FORMAT_PCM,
                sample_rate: 0,
                byte_rate: 0,
                block_align: 0,
                bits_per_sample: 0,
            }
        }
    }

    // The parser is just a wrapper around a ByteStream containere the
    // bytes the user passed in.
    pub struct WaveFileParser {
        byte_stream: ByteStream,
    }

    impl WaveFileParser {
        // The parsing is inspired by recursive descent parsers, but not nearly as clever.
        // (a) There is a method for each chunk defined in the '.wav' file specification
        // (b) There are helper methods for parsing the next chunk of an expected type.

        pub fn parse(bytes: Vec<u8>) -> WaveFile {
            let mut parser = WaveFileParser {
                byte_stream: ByteStream::new(bytes)
            };

            let mut wave_file: WaveFile = Default::default();

            if !parser.try_read(b"RIFF") {
                panic!("error: not a .wav file");
            }
            // Read the size of the "RIFF" chunk
            parser.read_chunk_size();

            // odd, this is not the "WAVE" character code we expected
            if !parser.try_read(b"WAVE") {
                panic!("error: RIFF chunk did not start with 'WAVE' character code")
            }

            parser.read_wave_riff_form(&mut wave_file);

            wave_file
        }

        fn read_wave_riff_form(&mut self, wave_file: &mut WaveFile) {
            let end_riff_chunk = self.byte_stream.bytes.len();

            // required fmt chunk
            if !self.try_accept_chunk(b"fmt ", end_riff_chunk) {
                panic!("error: could not find fmt chunk");
            }
            self.read_fmt_chunk(wave_file);

            // optional chunks
            if self.try_accept_chunk(b"fact", end_riff_chunk) {
                self.read_fact_chunk(wave_file);
            }

            if self.try_accept_chunk(b"cue ", end_riff_chunk) {
                self.read_cue_chunk(wave_file);
            }

            if self.try_accept_chunk(b"plst", end_riff_chunk) {
                self.read_playlist_chunk(wave_file);
            }

            if self.try_accept_list_type(b"adtl", end_riff_chunk) {
                if !self.try_accept_list_type(b"labl", end_riff_chunk) {
                    panic!("error: could not find labl chunk")
                }
                self.skip_unimplemented_chunk();
                if !self.try_accept_list_type(b"note", end_riff_chunk) {
                    panic!("error: could not find note chunk")
                }
                self.skip_unimplemented_chunk();
                if !self.try_accept_list_type(b"ltxt", end_riff_chunk) {
                    panic!("error: could not find ltxt chunk")
                }
                self.skip_unimplemented_chunk();
                if !self.try_accept_list_type(b"file", end_riff_chunk) {
                    panic!("error: could not find file chunk")
                }
                self.skip_unimplemented_chunk();
            }

            // Wave data can be either a LIST chunk with a 'wavl' list type or
            // a 'data' chunk
            if self.try_accept_list_type(b"wavl", end_riff_chunk) {
                let list_size = self.read_chunk_size();
                let end_list_chunk = self.byte_stream.offset + list_size as usize;

                // We know the list_type must be wavl, no need to check
                self.byte_stream.read(BYTES_LIST_TYPE);

                // The contents of a 'wavl` list can be a combination of data and slnt chunks
                while self.byte_stream.offset < end_list_chunk && !self.byte_stream.eof() {
                    if self.try_read(b"data") {
                        self.read_wave_data_chunk(wave_file);
                    }
                    else if self.try_read(b"slnt") {
                        self.read_wave_slnt_chunk(wave_file);
                    }
                }
            }
            else if self.try_accept_chunk(b"data", end_riff_chunk) {
                self.read_wave_data_chunk(wave_file);
            }
            else {
                panic!("error: could not find 'data' chunk or 'wavl' list type");
            }        
        }

        fn read_fmt_chunk(&mut self, wave_file: &mut WaveFile) {
            // We don't need the size value. 
            // We can visually inspect and see that the data size is even
            self.read_chunk_size();

            // wFormatTag
            let mut bytes_read = self.byte_stream.read(2);;
            bytes_read.reverse();
            let w_format_tag = to_u16(&bytes_read);

            // wChannels
            let mut bytes_read = self.byte_stream.read(2);
            bytes_read.reverse();
            let w_channels = to_u16(&bytes_read);

            // dwSamplesPerSec
            let mut bytes_read = self.byte_stream.read(4);
            bytes_read.reverse();
            let dw_samples_per_second = to_u32(&bytes_read);

            // dwAverageBytesPerSec
            let mut bytes_read = self.byte_stream.read(4);
            bytes_read.reverse();
            let dw_average_bytes_per_second = to_u32(&bytes_read);

            // wBlockAlign
            let mut bytes_read = self.byte_stream.read(2);
            bytes_read.reverse();
            let w_block_align = to_u16(&bytes_read);

            // wBitsPerSample
            let mut bytes_read = self.byte_stream.read(2);
            bytes_read.reverse();
            let w_bits_per_sample = to_u16(&bytes_read);

            // populate the wave file structure
            wave_file.channels = vec![vec![]; w_channels as usize];
            wave_file.sample_rate = dw_samples_per_second;
            wave_file.byte_rate = dw_average_bytes_per_second;
            wave_file.block_align = w_block_align;
            wave_file.bits_per_sample = w_bits_per_sample;
            if w_format_tag == WaveFormatCategory::WAVE_FORMAT_PCM as u16 {
                wave_file.wave_format = WaveFormatCategory::WAVE_FORMAT_PCM;
            } else {
                panic!("error: only PCM wave format is supported");
            }
        }

        fn read_fact_chunk(&mut self, wave_file: &mut WaveFile) {
            self.skip_unimplemented_chunk();
        }
        fn read_cue_chunk(&mut self, wave_file: &mut WaveFile) {
            self.skip_unimplemented_chunk();
        }
        fn read_playlist_chunk(&mut self, wave_file: &mut WaveFile) {
            self.skip_unimplemented_chunk();
        }

        fn read_wave_data_chunk(&mut self, wave_file: &mut WaveFile) {
            let size = self.read_chunk_size();
            let end_data = self.byte_stream.offset + size as usize;

            while self.byte_stream.offset < end_data {
                if wave_file.channels.len() == 1 {
                    // mono = 1 channel
                    let sample = self.read_sample(wave_file.bits_per_sample);

                    wave_file.channels[0].push(sample);
                } else if wave_file.channels.len() == 2 {
                    // stereo = 2 channels
                    let first_sample = self.read_sample(wave_file.bits_per_sample);
                    let second_sample = self.read_sample(wave_file.bits_per_sample);

                    wave_file.channels[0].push(first_sample);
                    wave_file.channels[1].push(second_sample);
                } else {
                    panic!("error: unsupported number of channels");
                }
            }

            // Make sure the offset is an even number at the end
            if self.byte_stream.offset % 2 != 0 {
                self.byte_stream.read(1);
            }
        }
        
        fn read_wave_slnt_chunk(&mut self, wave_file: &mut WaveFile) {
            self.skip_unimplemented_chunk();
        }

        fn read_sample(&mut self, bit_depth: u16) -> Sample {
            if bit_depth <= 8 {
                Sample::BitDepth8(self.byte_stream.read(1)[0])
            }
            else if bit_depth <= 16 {
                let mut bytes_read = self.byte_stream.read(2);
                bytes_read.reverse();

                Sample::BitDepth16(to_i16(&bytes_read))
            }
            else {
                panic!("error: unsupported bit-depth");
            }
        }

        // Utility Methods
        // try_read: To match subsequent bytes to `expected`. Returns true if successful
        // try_accept_chunk:
        // try_accept_list_type: 
        // skip_unrecognized_chunk: 

        // Attempts to match the subsequent bytes to `expected` 
        // A successful match will result in moving ahead in the byte stream
        // A failed match will keep our position unchanged.
        fn try_read(&mut self, expected: &[u8]) -> bool {
            let count = expected.len();
            let bytes = self.byte_stream.peek(count);

            if expected == &bytes[..] {
                self.byte_stream.read(count);
                true
            } else {
                false
            }
        }

        // Notes:
        // The structure of a riff file is supposeed to be backwards compatible. So the specifications says to ignore unrecognized chunk_ids.
        // The chunk we expect to be next might actually come after a chunk we don't recognize. 

        // Attempts to match `chunk_id`.
        // The chunk we are looking for does not have to be the very next one in the byte stream. 
        // We will skip over any chunks that don't match until we 
        // (a) find the chunk we are looking for or 
        // (b) get to `parent_chunk_end`
        fn try_accept_chunk(&mut self, chunk_id: &[u8], parent_chunk_end: usize) -> bool {
            if parent_chunk_end > self.byte_stream.bytes.len() {
                panic!("error: parent_chunk_end cannot be greater than the length of the underlying byte array");
            }

            if chunk_id.len() != BYTES_CHUNK_ID {
                panic!("error: chunk_id does not have the expected length");
            }

            let start_offset = self.byte_stream.offset;
            let num_bytes_to_read = parent_chunk_end - start_offset;

            let mut num_bytes_read: usize = 0;
            let mut found = false;

            while !found && num_bytes_read < num_bytes_to_read && !self.byte_stream.eof() {
                let bytes = self.byte_stream.read(BYTES_CHUNK_ID);
                num_bytes_read += BYTES_CHUNK_ID;

                if chunk_id == &bytes[..] {
                    found = true;
                } else {
                    // Skip over the unrecognized chunk
                    let mut chunk_size = self.read_chunk_size();
                    num_bytes_read += BYTES_CHUNK_SIZE;

                    if chunk_size % 2 != 0 {
                        chunk_size += 1;
                    }

                    self.byte_stream.read(chunk_size as usize);
                    num_bytes_read += chunk_size as usize;
                }
            }

            if found {
                true
            } else {
                // Rewind to start
                self.byte_stream.seek(start_offset);
                false
            }
        }

        // We attempt to match a LIST chunk with the given `list_type` 
        // The matching is done in a similar manner to `try_accept_chunk`
        fn try_accept_list_type(&mut self, list_type: &[u8], parent_chunk_end: usize) -> bool {
            if parent_chunk_end > self.byte_stream.bytes.len() {
                panic!("error: parent_chunk_end cannot be greater than the length of the underlying byte array");
            }

            if list_type.len() != BYTES_LIST_TYPE {
                panic!("error: chunk_id does not have the expected length");
            }

            let mut found = false;
            let start_offset = self.byte_stream.offset;

            while self.try_accept_chunk(b"LIST", parent_chunk_end) {
                // Get the list chunk size
                let mut list_size = self.read_chunk_size();
                // Get the list type
                let lt = self.byte_stream.read(BYTES_LIST_TYPE);
                if &lt[..] == list_type {
                    found = true;
                } else {
                    // Not the list we are looking for :/ Skip over it
                    if list_size % 2 != 0 {
                        list_size += 1;
                    }
                    self.byte_stream.read(list_size as usize);
                }
            }

            if found {
                // Rewind to the begining of the LIST chunk. This will allow later methods to have access to the length of the list
                let before_list_chunk = self.byte_stream.offset - (BYTES_LIST_TYPE + BYTES_CHUNK_SIZE);
                self.byte_stream.seek(before_list_chunk);
                true
            } else {
                // Rewind to start
                self.byte_stream.seek(start_offset);
                false
            }
        }

        // Read the chunk size field as 32 bit unsigned integer.
        // Will handle flipping the bytes since .wav files are in little-endian form
        fn read_chunk_size(&mut self) -> u32 {
            // Bytes are in little-endian order.
            let mut bytes_read = self.byte_stream.read(BYTES_CHUNK_SIZE);
            bytes_read.reverse();

            let size = to_u32(&bytes_read);

            size
        }

        // Placeholder
        fn skip_unimplemented_chunk(&mut self) {
            let mut size = self.read_chunk_size();
            if !(size % 2 == 0) {
                size += 1;
            }

            self.byte_stream.read(size as usize);
        }
    }

    // A wrapper around a sequence of bytes with an offset
    // This makes it easy to move back and forth in the stream of bytes as we parse it.
    struct ByteStream {
        bytes: Vec<u8>,
        offset: usize,
    }

    impl ByteStream {
        fn new(b: Vec<u8>) -> ByteStream {
            // The offset acts like a movable pointer to a location in the byte sequence
            // It starts off at 0
            ByteStream {
                bytes: b,
                offset: 0,
            }
        }

        // EOF = End of File
        // Simple check to see if we are at the end of the byte sequence
        fn eof(&self) -> bool {
            self.offset == self.bytes.len()
        }

        // Read the next `count` bytes and update the offset
        fn read(&mut self, count: usize) -> Vec<u8> {
            let bytes_read = self.peek(count);

            // A read updates the offset
            self.offset = self.offset + count;

            bytes_read
        }

        // Read the next `count` bytes
        fn peek(&self, count: usize) -> Vec<u8> {
            let start = self.offset;
            let end = self.offset + count;

            let ret = match self.bytes.get(start..end) {
                Some(x) => x,
                None => panic!("index out of bounds"),
            };

            ret.to_vec()
        }

        // Change the value of the offset to `offset`
        // The next call to read or seek will start from this new value.
        fn seek(&mut self, offset: usize) {
            if offset >= self.bytes.len() {
                panic!("error: index out of bounds");
            } else {
                self.offset = offset;
            }
        }
    }


    fn to_u32(list: &[u8]) -> u32 {
        assert_eq!(4, list.len());

        (list[0] as u32) << 24 | (list[1] as u32) << 16 | (list[2] as u32) << 8 | list[3] as u32
    }

    fn to_u16(list: &[u8]) -> u16 {
        assert_eq!(2, list.len());

        (list[0] as u16) << 8 | list[1] as u16
    }

    fn to_i16(list: &[u8]) -> i16 {
        to_u16(list) as i16
    }
}

#[cfg(test)]
mod unit_tests {
    use std::fs;
    use crate::wave;

    #[test]
    fn test_parsing_simple_wav_file() {
        let wave_file = wave::WaveFileParser::parse(fs::read("/home/eze/Audio/3seconds1channel8bit8000Hz.wav").unwrap());

        assert_eq!(1, wave_file.channels.len());
        assert_eq!(8, wave_file.bits_per_sample);
        assert_eq!(8000, wave_file.sample_rate);
    }
    #[test]
    fn test_parsing_two_channel_audio() {
        let wave_file = wave::WaveFileParser::parse(fs::read("/home/eze/Audio/3seconds2channels8bit8000Hz.wav").unwrap());

        assert_eq!(2, wave_file.channels.len());
        assert_eq!(8, wave_file.bits_per_sample);
        assert_eq!(8000, wave_file.sample_rate);
    }

    #[test]
    fn test_parsing_16000_sample_rate() {
        let wave_file = wave::WaveFileParser::parse(fs::read("/home/eze/Audio/3seconds1channel8bit16000Hz.wav").unwrap());

        assert_eq!(1, wave_file.channels.len());
        assert_eq!(8, wave_file.bits_per_sample);
        assert_eq!(16000, wave_file.sample_rate);

    }

    #[test]
    fn test_parsing_16bit_samples() {
        let wave_file = wave::WaveFileParser::parse(fs::read("/home/eze/Audio/3seconds1channel16bit8000Hz.wav").unwrap());

        assert_eq!(1, wave_file.channels.len());
        assert_eq!(16, wave_file.bits_per_sample);
        assert_eq!(8000, wave_file.sample_rate);
    }

    #[test]
    fn test_parsing_wave_file_with_metadata() {
        let wave_file = wave::WaveFileParser::parse(fs::read("/home/eze/Audio/3seconds1channel16bit8000HzWithMetadata.wav").unwrap());
    }
}
