//! Tiny static ELF64 payload used only by the opt-in QEMU Ring 3 smoke test.
//! It calls int 0x80 SYS_WRITE(fd=1, "FLX USERSPACE: PASS\\n") and then spins.

const ELF_SIZE: usize = 0x1000 + 48;

/// A minimal x86_64 ET_EXEC image with one RX PT_LOAD segment at 0x400000.
const fn build_elf() -> [u8; ELF_SIZE] {
    let mut image = [0u8; ELF_SIZE];

    // ELF64 little-endian identification and executable header.
    image[0] = 0x7f; image[1] = b'E'; image[2] = b'L'; image[3] = b'F';
    image[4] = 2; image[5] = 1; image[6] = 1;
    put_u16(&mut image, 16, 2);       // ET_EXEC
    put_u16(&mut image, 18, 62);      // EM_X86_64
    put_u32(&mut image, 20, 1);       // EV_CURRENT
    put_u64(&mut image, 24, 0x400000); // entry
    put_u64(&mut image, 32, 64);      // program header table offset
    put_u16(&mut image, 52, 64);      // ELF header size
    put_u16(&mut image, 54, 56);      // program header size
    put_u16(&mut image, 56, 1);       // program header count

    // PT_LOAD: file offset 0x1000 -> virtual address 0x400000, RX.
    put_u32(&mut image, 64, 1);
    put_u32(&mut image, 68, 5);
    put_u64(&mut image, 72, 0x1000);
    put_u64(&mut image, 80, 0x400000);
    put_u64(&mut image, 96, 48);      // file size
    put_u64(&mut image, 104, 48);     // memory size
    put_u64(&mut image, 112, 0x1000); // alignment

    // mov eax, 1; mov edi, 1; lea rsi, [rip + 9]; mov edx, 20; int 0x80; jmp $.
    let code: [u8; 28] = [
        0xB8, 1, 0, 0, 0,
        0xBF, 1, 0, 0, 0,
        0x48, 0x8D, 0x35, 9, 0, 0, 0,
        0xBA, 20, 0, 0, 0,
        0xCD, 0x80,
        0xEB, 0xFE,
    ];
    let mut i = 0;
    while i < code.len() {
        image[0x1000 + i] = code[i];
        i += 1;
    }
    let message: [u8; 20] = *b"FLX USERSPACE: PASS\\n";
    i = 0;
    while i < message.len() {
        image[0x1000 + 28 + i] = message[i];
        i += 1;
    }
    image
}

const fn put_u16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at] = value as u8;
    bytes[at + 1] = (value >> 8) as u8;
}
const fn put_u32(bytes: &mut [u8], at: usize, value: u32) {
    let mut i = 0;
    while i < 4 { bytes[at + i] = (value >> (i * 8)) as u8; i += 1; }
}
const fn put_u64(bytes: &mut [u8], at: usize, value: u64) {
    let mut i = 0;
    while i < 8 { bytes[at + i] = (value >> (i * 8)) as u8; i += 1; }
}

static ELF_BYTES: [u8; ELF_SIZE] = build_elf();
pub const ELF: &[u8] = &ELF_BYTES;
