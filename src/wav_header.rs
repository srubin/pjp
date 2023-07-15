#[derive(Debug, Copy, Clone)]
pub struct WavHeader {
    pub riff: [u8; 4],
    pub file_size: u32,
    pub file_type: [u8; 4],
    pub format_chunk_marker: [u8; 4],
    pub format_data_length: u32, // should be 16 for PCM
    pub format_type: u16,
    pub number_of_channels: u16,
    pub sample_rate: u32,
    pub bytes_per_second: u32,
    pub bytes_per_frame: u16,
    pub bits_per_sample: u16,
    pub data_chunk_marker: [u8; 4],
    pub data_size: u32,
}

fn find_chunk(bytes: &Vec<u8>, start: usize, name: &[u8]) -> Option<usize> {
    for i in start..bytes.len() {
        for j in 0..name.len() {
            if bytes[i + j] != name[j] {
                break;
            }
            if j == name.len() - 1 {
                return Some(i);
            }
        }
    }
    return None;
}

impl WavHeader {
    pub fn from(header_bytes: Vec<u8>) -> WavHeader {
        assert!(
            header_bytes.len() >= 44,
            "WavHeader should be at least 44 bytes, but was {}",
            header_bytes.len()
        );

        let format_data_length = u32::from_le_bytes([
            header_bytes[16],
            header_bytes[17],
            header_bytes[18],
            header_bytes[19],
        ]);

        let data_chunk_start = find_chunk(
            &header_bytes,
            20 + format_data_length as usize,
            "data".as_bytes(),
        );

        if data_chunk_start.is_none() {
            panic!("Could not find data chunk in wav file");
        }

        let data_chunk_start = data_chunk_start.unwrap();

        let needed_byte_len = data_chunk_start + 8;

        assert!(
            header_bytes.len() >= needed_byte_len,
            "WavHeader should be at least {} bytes, but was {}",
            needed_byte_len,
            header_bytes.len()
        );

        // read data from the header buffer into a WavHeader struct
        let header = WavHeader {
            riff: [
                header_bytes[0],
                header_bytes[1],
                header_bytes[2],
                header_bytes[3],
            ],
            file_size: u32::from_le_bytes([
                header_bytes[4],
                header_bytes[5],
                header_bytes[6],
                header_bytes[7],
            ]),
            file_type: [
                header_bytes[8],
                header_bytes[9],
                header_bytes[10],
                header_bytes[11],
            ],
            format_chunk_marker: [
                header_bytes[12],
                header_bytes[13],
                header_bytes[14],
                header_bytes[15],
            ],
            format_data_length,
            format_type: u16::from_le_bytes([header_bytes[20], header_bytes[21]]),
            number_of_channels: u16::from_le_bytes([header_bytes[22], header_bytes[23]]),
            sample_rate: u32::from_le_bytes([
                header_bytes[24],
                header_bytes[25],
                header_bytes[26],
                header_bytes[27],
            ]),
            bytes_per_second: u32::from_le_bytes([
                header_bytes[28],
                header_bytes[29],
                header_bytes[30],
                header_bytes[31],
            ]),
            bytes_per_frame: u16::from_le_bytes([header_bytes[32], header_bytes[33]]),
            bits_per_sample: u16::from_le_bytes([header_bytes[34], header_bytes[35]]),
            data_chunk_marker: [
                header_bytes[data_chunk_start + 0],
                header_bytes[data_chunk_start + 1],
                header_bytes[data_chunk_start + 2],
                header_bytes[data_chunk_start + 3],
            ],
            data_size: u32::from_le_bytes([
                header_bytes[data_chunk_start + 4],
                header_bytes[data_chunk_start + 5],
                header_bytes[data_chunk_start + 6],
                header_bytes[data_chunk_start + 7],
            ]),
        };
        header
    }

    pub fn data_start(&self) -> usize {
        20 + self.format_data_length as usize + 8
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_wav_header_from_bytes() {
        let header_vec = [
            82, 73, 70, 70, 60, 5, 5, 0, 87, 65, 86, 69, 102, 109, 116, 32, 18, 0, 0, 0, 1, 0, 1,
            0, 68, 172, 0, 0, 136, 88, 1, 0, 2, 0, 16, 0, 0, 0, 100, 97, 116, 97, 22, 5, 5, 0,
        ]
        .to_vec();

        // convert u8 slice to [u8; 44] array
        // let mut header_bytes = header_bytes.to_owned();

        let header = super::WavHeader::from(header_vec);
        assert_eq!(header.sample_rate, 44100);
        assert_eq!(header.format_type, 1);
        assert_eq!(header.number_of_channels, 1);
        assert_eq!(header.bits_per_sample, 16);
        assert_eq!(header.data_size, 328982);
    }
}
