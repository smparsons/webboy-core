use utils::{calculate_left_stereo_sample, calculate_right_stereo_sample};

use crate::apu::envelope::should_disable_dac;
use crate::apu::noise::{initialize_noise_channel, NoiseChannel};
use crate::apu::wave::{initialize_wave_channel, WaveChannel};
use crate::apu::pulse::{initialize_pulse_channel, PulseChannel};
use crate::apu::utils::bounded_wrapping_add;
use crate::emulator::Emulator;
use crate::utils::{get_bit, is_bit_set, reset_bit, set_bit};

#[derive(Debug)]
pub struct ApuState {
    pub audio_master_control: u8,
    pub sound_panning: u8,
    pub master_volume: u8,
    pub channel1: PulseChannel,
    pub channel2: PulseChannel,
    pub channel3: WaveChannel,
    pub channel4: NoiseChannel,
    pub divider_apu: u8,
    pub last_divider_time: u8,
    pub instruction_cycles: u8,
    pub left_sample_queue: Vec<f32>,
    pub right_sample_queue: Vec<f32>
}

pub fn initialize_apu() -> ApuState {
    ApuState {
        audio_master_control: 0,
        sound_panning: 0,
        master_volume: 0,
        channel1: initialize_pulse_channel(),
        channel2: initialize_pulse_channel(),
        channel3: initialize_wave_channel(),
        channel4: initialize_noise_channel(),
        divider_apu: 0,
        last_divider_time: 0,
        instruction_cycles: 0,
        left_sample_queue: Vec::new(),
        right_sample_queue: Vec::new()
    }
}

const CH1_ENABLED_INDEX: u8 = 0;
const CH2_ENABLED_INDEX: u8 = 1;
const CH3_ENABLED_INDEX: u8 = 2;
const CH4_ENABLED_INDEX: u8 = 3;
const CH3_DAC_ENABLED_INDEX: u8 = 7;
const APU_ENABLED_INDEX: u8 = 7;
const MAX_DIV_APU_STEPS: u8 = 7;

const CPU_RATE: u32 = 4194304;
const SAMPLE_RATE: u32 = 48000;
const ENQUEUE_RATE: u32 = CPU_RATE / SAMPLE_RATE;

fn should_step_div_apu(emulator: &mut Emulator) -> bool {
    emulator.apu.last_divider_time > 0
        && emulator.timers.divider > 0
        && get_bit(emulator.apu.last_divider_time, 4) == 1
        && get_bit(emulator.timers.divider, 4) == 0
}

fn step_div_apu(emulator: &mut Emulator) {
    if should_step_div_apu(emulator) {
        let current_divider_apu = emulator.apu.divider_apu;

        let envelope_step = 7;
        let length_steps = vec![0, 2, 4, 6];
        let sweep_steps = vec![2, 6];

        if current_divider_apu == envelope_step {
            pulse::step_envelope(&mut emulator.apu.channel1);
            pulse::step_envelope(&mut emulator.apu.channel2);
            noise::step_envelope(&mut emulator.apu.channel4); 
        }

        if length_steps.contains(&current_divider_apu) {
            pulse::step_length(&mut emulator.apu.channel1);
            pulse::step_length(&mut emulator.apu.channel2);
            wave::step_length(&mut emulator.apu.channel3);
            noise::step_length(&mut emulator.apu.channel4);
        }
        
        if sweep_steps.contains(&current_divider_apu) {
            pulse::step_sweep(&mut emulator.apu.channel1);
        }

        if !emulator.apu.channel1.enabled && is_bit_set(emulator.apu.audio_master_control, CH1_ENABLED_INDEX) {
            emulator.apu.audio_master_control = reset_bit(emulator.apu.audio_master_control, CH1_ENABLED_INDEX);
        }
        if !emulator.apu.channel2.enabled && is_bit_set(emulator.apu.audio_master_control, CH2_ENABLED_INDEX) {
            emulator.apu.audio_master_control = reset_bit(emulator.apu.audio_master_control, CH2_ENABLED_INDEX);
        }
        if !emulator.apu.channel3.enabled && is_bit_set(emulator.apu.audio_master_control, CH3_ENABLED_INDEX) {
            emulator.apu.audio_master_control = reset_bit(emulator.apu.audio_master_control, CH3_ENABLED_INDEX);
        }
        if !emulator.apu.channel4.enabled && is_bit_set(emulator.apu.audio_master_control, CH4_ENABLED_INDEX) {
            emulator.apu.audio_master_control = reset_bit(emulator.apu.audio_master_control, CH4_ENABLED_INDEX);
        }

        emulator.apu.divider_apu = bounded_wrapping_add(emulator.apu.divider_apu, MAX_DIV_APU_STEPS)
    }

    emulator.apu.last_divider_time = emulator.timers.divider;
}

fn apu_enabled(audio_master_control: u8) -> bool {
    is_bit_set(audio_master_control, APU_ENABLED_INDEX)
}

fn enqueue_audio_samples(emulator: &mut Emulator) {
    if emulator.apu.instruction_cycles as u32 >= ENQUEUE_RATE {
        emulator.apu.instruction_cycles = 0;

        let sound_panning = emulator.apu.sound_panning;

        let channel1_output = pulse::dac_output(&emulator.apu.channel1);
        let channel2_output = pulse::dac_output(&emulator.apu.channel2);
        let channel3_output = wave::dac_output(&emulator);
        let channel4_output = noise::dac_output(&emulator.apu.channel4);

        let left_master_volume = (emulator.apu.master_volume & 0b01110000) >> 4;

        let left_sample = calculate_left_stereo_sample(sound_panning,
            left_master_volume,
            channel1_output,
            channel2_output,
            channel3_output,
            channel4_output);

        emulator.apu.left_sample_queue.push(left_sample);

        let right_master_volume = emulator.apu.master_volume & 0b111;

        let right_sample = calculate_right_stereo_sample(sound_panning,
            right_master_volume,
            channel1_output,
            channel2_output,
            channel3_output,
            channel4_output);

        emulator.apu.right_sample_queue.push(right_sample);
    }
}

pub fn step(emulator: &mut Emulator) {    
    if apu_enabled(emulator.apu.audio_master_control) {
        let instruction_clock_cycles = emulator.cpu.clock.instruction_clock_cycles;
        emulator.apu.instruction_cycles += instruction_clock_cycles;
        pulse::step(&mut emulator.apu.channel1, instruction_clock_cycles);
        pulse::step(&mut emulator.apu.channel2, instruction_clock_cycles);
        wave::step(&mut emulator.apu.channel3, instruction_clock_cycles);
        noise::step(&mut emulator.apu.channel4, instruction_clock_cycles);
        step_div_apu(emulator);
        enqueue_audio_samples(emulator);
    }    
}

pub fn set_ch1_period_high(emulator: &mut Emulator, new_period_high_value: u8) {
    emulator.apu.channel1.period.high = new_period_high_value;
    
    if pulse::should_trigger(&emulator.apu.channel1) { 
        pulse::trigger(&mut emulator.apu.channel1, true);
        emulator.apu.audio_master_control = set_bit(emulator.apu.audio_master_control, CH1_ENABLED_INDEX);
    }
}

pub fn set_ch2_period_high(emulator: &mut Emulator, new_period_high_value: u8) {
    emulator.apu.channel2.period.high = new_period_high_value;
    
    if pulse::should_trigger(&emulator.apu.channel2) { 
        pulse::trigger(&mut emulator.apu.channel2, false);
        emulator.apu.audio_master_control = set_bit(emulator.apu.audio_master_control, CH2_ENABLED_INDEX);
    }
}

pub fn set_ch3_period_high(emulator: &mut Emulator, new_period_high_value: u8) {
    emulator.apu.channel3.period.high = new_period_high_value;

    if wave::should_trigger(&emulator.apu.channel3) {
        wave::trigger(&mut emulator.apu.channel3);
        emulator.apu.audio_master_control = set_bit(emulator.apu.audio_master_control, CH3_ENABLED_INDEX);
    }
}

pub fn set_ch4_control(emulator: &mut Emulator, new_control_value: u8) {
    emulator.apu.channel4.control = new_control_value;

    if noise::should_trigger(&emulator.apu.channel4) {
        noise::trigger(&mut emulator.apu.channel4);
        emulator.apu.audio_master_control = set_bit(emulator.apu.audio_master_control, CH4_ENABLED_INDEX);
    }
}

pub fn set_ch1_envelope_settings(emulator: &mut Emulator, new_envelope_settings: u8) {
    emulator.apu.channel1.envelope.initial_settings = new_envelope_settings;

    let should_disable = should_disable_dac(&emulator.apu.channel1.envelope);

    emulator.apu.channel1.dac_enabled = !should_disable;

    if should_disable {
        pulse::disable(&mut emulator.apu.channel1); 
        emulator.apu.audio_master_control = reset_bit(emulator.apu.audio_master_control, CH1_ENABLED_INDEX);
    }
}

pub fn set_ch2_envelope_settings(emulator: &mut Emulator, new_envelope_settings: u8) {
    emulator.apu.channel2.envelope.initial_settings = new_envelope_settings;

    let should_disable = should_disable_dac(&emulator.apu.channel2.envelope);

    emulator.apu.channel2.dac_enabled = !should_disable;

    if should_disable {
        pulse::disable(&mut emulator.apu.channel2); 
        emulator.apu.audio_master_control = reset_bit(emulator.apu.audio_master_control, CH2_ENABLED_INDEX);
    }
}

pub fn set_ch3_dac_enabled(emulator: &mut Emulator, new_dac_enabled_register_value: u8) {
    let should_disable = !is_bit_set(new_dac_enabled_register_value, CH3_DAC_ENABLED_INDEX);

    emulator.apu.channel3.dac_enabled = !should_disable;
    
    if should_disable {
        wave::disable(&mut emulator.apu.channel3);
        emulator.apu.audio_master_control = reset_bit(emulator.apu.audio_master_control, CH3_ENABLED_INDEX);
    }
}

pub fn set_ch4_envelope_settings(emulator: &mut Emulator, new_envelope_settings: u8) {
    emulator.apu.channel4.envelope.initial_settings = new_envelope_settings;

    let should_disable = should_disable_dac(&emulator.apu.channel4.envelope);

    emulator.apu.channel4.dac_enabled = !should_disable;

    if should_disable {
        noise::disable(&mut emulator.apu.channel4);
        emulator.apu.audio_master_control = reset_bit(emulator.apu.audio_master_control, CH4_ENABLED_INDEX);
    }
}

pub fn set_audio_master_control(emulator: &mut Emulator, new_audio_master_control: u8) {
    emulator.apu.audio_master_control = new_audio_master_control & 0b11110000;

    let is_powered_off = !is_bit_set(emulator.apu.audio_master_control, APU_ENABLED_INDEX);

    if is_powered_off {
        emulator.apu = initialize_apu();
    }
}

#[cfg(test)]
mod tests;

pub mod pulse;
pub mod wave;
pub mod noise;
pub mod length;
pub mod sweep;
mod envelope;
mod period;
mod utils;
