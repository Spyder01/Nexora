use core::option::Option;


#[derive(Debug, Copy, Clone)]
pub struct BitSet64<const W: usize> {
    words: [u64; W]
}

impl<const W: usize> BitSet64<W> {
      pub const EMPTY: Self = BitSet64 { words: [0u64; W] };

      pub fn clear(&mut self, bit: usize) {
          let word  = bit >> 6;      
          let shift = bit & 63;      
          self.words[word] &= !(1u64 << shift)
      }

      pub fn set(&mut self, bit: usize) {
          let word = bit >> 6;
          let shift = bit & 63;

          self.words[word] |= 1u64 << shift;
      }

      pub fn get(self, bit: usize) -> bool {
          let word = bit >> 6;
          let shift = bit & 63;

          self.words[word] & (1u64 << shift) != 0
      }

      pub fn first_set(&self) -> Option<usize> {
          for i in 0..W {
              if self.words[i] != 0 {
                  return Some((i << 6) + self.words[i].trailing_zeros() as usize);
              }
          }

          None
      }
}
