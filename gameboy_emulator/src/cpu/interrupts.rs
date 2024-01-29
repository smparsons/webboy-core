use crate::cpu::jumps;
use crate::emulator::Emulator;

pub enum InterruptType {
    VBlank,
    LCDStatus,
    TimerOverflow,
    SerialLink,
    JoypadPress
}

#[derive(Debug)]
pub struct InterruptRegisters {
    pub enabled: u8,
    pub flags: u8
}

fn get_fired_interrupt_bits(emulator: &Emulator) -> u8 {
    emulator.interrupts.enabled & emulator.interrupts.flags & 0x1F
}

fn get_fired_interrupt(emulator: &Emulator) -> Option<InterruptType> {
    let fired_interrupt_bits = get_fired_interrupt_bits(emulator);
    if (fired_interrupt_bits & 0x01) != 0 {
        Some(InterruptType::VBlank)
    }
    else if (fired_interrupt_bits & 0x02) != 0 {
        Some(InterruptType::LCDStatus)
    }
    else if (fired_interrupt_bits & 0x04) != 0 {
        Some(InterruptType::TimerOverflow)
    }
    else if (fired_interrupt_bits & 0x08) != 0 {
        Some(InterruptType::SerialLink)
    }
    else if (fired_interrupt_bits & 0x10) != 0 {
        Some(InterruptType::JoypadPress)
    }
    else {
        None
    }
}

fn get_interrupt_isr(interrupt_type: &InterruptType) -> u8 {
    match interrupt_type {
        InterruptType::VBlank => 0x40,
        InterruptType::LCDStatus => 0x48,
        InterruptType::TimerOverflow => 0x50,
        InterruptType::SerialLink => 0x58,
        InterruptType::JoypadPress => 0x60
    }
}

fn turn_off_interrupt_flag(emulator: &mut Emulator, interrupt_type: &InterruptType) {
    let interrupt_registers = &mut emulator.interrupts;
    match interrupt_type {
        InterruptType::VBlank =>
            interrupt_registers.flags = interrupt_registers.flags & !0x01,
        InterruptType::LCDStatus =>
            interrupt_registers.flags = interrupt_registers.flags & !0x02,
        InterruptType::TimerOverflow =>
            interrupt_registers.flags = interrupt_registers.flags & !0x04,
        InterruptType::SerialLink =>
            interrupt_registers.flags = interrupt_registers.flags & !0x08,
        InterruptType::JoypadPress =>
            interrupt_registers.flags = interrupt_registers.flags & !0x10
    }
}

pub fn interrupts_fired(emulator: &Emulator) -> bool {
    let fired_interrupt_bits = get_fired_interrupt_bits(emulator);
    fired_interrupt_bits != 0
}

pub fn step(emulator: &mut Emulator) {
    emulator.cpu.clock.instruction_clock_cycles = 0;
    if emulator.cpu.interrupts.enabled && interrupts_fired(emulator) {
        let maybe_fired_interrupt = get_fired_interrupt(emulator);
        match maybe_fired_interrupt {
            Some(interrupt_type) => {
                emulator.cpu.interrupts.enabled = false;
                turn_off_interrupt_flag(emulator, &interrupt_type);
                let isr_address = get_interrupt_isr(&interrupt_type);
                jumps::restart(emulator, isr_address as u16);
            },
            None => ()
        }
    }
}
