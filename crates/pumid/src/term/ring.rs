pub struct Ring {
    buf: Vec<u8>,
    capacity: usize,
    head: usize,
    end: u64,
}
impl Ring {
    pub fn new(capacity: usize) -> Ring {
        Ring { buf: Vec::new(), capacity: capacity.max(1024), head: 0, end: 0 }
    }
    pub fn with(capacity: usize, bytes: &[u8], end: u64) -> Ring {
        let mut ring = Ring::new(capacity);
        ring.push(bytes);
        ring.end = end.max(bytes.len() as u64);
        ring
    }
    pub fn push(&mut self, bytes: &[u8]) {
        self.end += bytes.len() as u64;
        let bytes = if bytes.len() > self.capacity { &bytes[bytes.len() - self.capacity..] } else { bytes };
        if self.buf.len() < self.capacity {
            let room = self.capacity - self.buf.len();
            let (now, rest) = bytes.split_at(bytes.len().min(room));
            self.buf.extend_from_slice(now);
            if rest.is_empty() {
                return;
            }
            self.write_wrapped(rest);
        } else {
            self.write_wrapped(bytes);
        }
    }
    fn write_wrapped(&mut self, bytes: &[u8]) {
        let first = bytes.len().min(self.capacity - self.head);
        self.buf[self.head..self.head + first].copy_from_slice(&bytes[..first]);
        self.buf[..bytes.len() - first].copy_from_slice(&bytes[first..]);
        self.head = (self.head + bytes.len()) % self.capacity;
    }
    pub fn end(&self) -> u64 {
        self.end
    }
    pub fn start(&self) -> u64 {
        self.end - self.buf.len() as u64
    }
    pub fn capacity(&self) -> usize {
        self.capacity
    }
    pub fn len(&self) -> usize {
        self.buf.len()
    }
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
    pub fn since(&self, seq: u64) -> Option<Vec<u8>> {
        if seq < self.start() || seq > self.end {
            return None;
        }
        Some(self.tail((self.end - seq) as usize))
    }
    pub fn tail(&self, n: usize) -> Vec<u8> {
        let n = n.min(self.buf.len());
        let mut out = Vec::with_capacity(n);
        if self.buf.len() < self.capacity {
            out.extend_from_slice(&self.buf[self.buf.len() - n..]);
        } else {
            let from = (self.head + self.capacity - n) % self.capacity;
            if from + n <= self.capacity {
                out.extend_from_slice(&self.buf[from..from + n]);
            } else {
                out.extend_from_slice(&self.buf[from..]);
                out.extend_from_slice(&self.buf[..n - (self.capacity - from)]);
            }
        }
        out
    }
    pub fn clear(&mut self) {
        self.buf.clear();
        self.head = 0;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keeps_the_tail_with_its_numbers() {
        let mut ring = Ring::new(1024);
        let data: Vec<u8> = (0..3000u32).map(|i| (i % 251) as u8).collect();
        for chunk in data.chunks(77) {
            ring.push(chunk);
        }
        assert_eq!(ring.end(), 3000);
        assert_eq!(ring.start(), 3000 - 1024);
        assert_eq!(ring.tail(1024), &data[3000 - 1024..]);
        assert_eq!(ring.tail(10), &data[2990..]);
        assert_eq!(ring.since(2500).unwrap(), &data[2500..]);
        assert!(ring.since(100).is_none());
        ring.push(&data);
        assert_eq!(ring.tail(1024), &data[3000 - 1024..]);
    }
}
