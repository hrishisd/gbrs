## basic functionality

- [x] implement OAM 
- [x] implement OAM DMA transfer
- [x] implement lyc compare interrupt
- [x] implement rendering background
- [x] implement rendering objects
- [x] implement support for 2-tile objects
- [x] implement rendering window
- [x] implement sdl2 frontend (testing)
- [x] unmap bootrom (set ff50 to nonzero value)
- [x] wrap in CLI and configure logging and framerate
- [x] implement the HALT instruction
- [ ] implement input handling
- [ ] implement the HALT bug
- [ ] Do a big refactor/cleanup with a single Emulator struct that owns all
  subcomponents, with subcomponents having references to eachother
- [ ] support rom banking
- [ ] double check interrupt handling
- [ ] implement the STOP instruction properly
- [ ] implement terminal frontend
