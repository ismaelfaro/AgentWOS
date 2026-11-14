// Tab Jam synthesizer. Works on any BaseAudioContext, so the same code plays
// live and renders offline for level checks.

const hz = (midi) => 440 * 2 ** ((midi - 69) / 12);

/** Generated impulse response: decaying stereo noise, darker towards the tail. */
function impulse(ctx, seconds) {
  const length = Math.floor(ctx.sampleRate * seconds);
  const buffer = ctx.createBuffer(2, length, ctx.sampleRate);
  for (let ch = 0; ch < 2; ch++) {
    const data = buffer.getChannelData(ch);
    let last = 0;
    for (let i = 0; i < length; i++) {
      const t = i / length;
      const white = Math.random() * 2 - 1;
      last += (white - last) * (1 - 0.75 * t); // one-pole lowpass that closes over time
      data[i] = last * (1 - t) ** 3;
    }
  }
  return buffer;
}

/** Soft clipper so peaks round off instead of crackling. */
function softClip(ctx, drive) {
  const shaper = ctx.createWaveShaper();
  const curve = new Float32Array(1024);
  for (let i = 0; i < curve.length; i++) {
    const x = (i / (curve.length - 1)) * 2 - 1;
    curve[i] = Math.tanh(x * drive) / Math.tanh(drive);
  }
  shaper.curve = curve;
  shaper.oversample = "2x";
  return shaper;
}

export function createSynth(ctx) {
  // master: bus -> compressor -> soft clip -> volume -> out
  const master = ctx.createGain();
  const compressor = ctx.createDynamicsCompressor();
  compressor.threshold.value = -16;
  compressor.ratio.value = 3;
  compressor.attack.value = 0.005;
  compressor.release.value = 0.18;
  const volume = ctx.createGain();
  volume.gain.value = 0.8;
  master.connect(compressor).connect(softClip(ctx, 1.4)).connect(volume).connect(ctx.destination);

  // shared reverb and a tempo-synced delay for the lead
  const reverb = ctx.createConvolver();
  reverb.buffer = impulse(ctx, 1.8);
  const reverbReturn = ctx.createGain();
  reverbReturn.gain.value = 0.5;
  reverb.connect(reverbReturn).connect(master);

  const delay = ctx.createDelay(1.5);
  const feedback = ctx.createGain();
  feedback.gain.value = 0.32;
  const delayTone = ctx.createBiquadFilter();
  delayTone.type = "lowpass";
  delayTone.frequency.value = 2200;
  const delayReturn = ctx.createGain();
  delayReturn.gain.value = 0.28;
  delay.connect(delayTone).connect(feedback).connect(delay);
  delayTone.connect(delayReturn).connect(master);

  // the bass dips slightly under each kick so the low end stays clear
  const bassBus = ctx.createGain();
  bassBus.connect(master);

  const noise = ctx.createBuffer(1, ctx.sampleRate, ctx.sampleRate);
  const noiseData = noise.getChannelData(0);
  for (let i = 0; i < noiseData.length; i++) noiseData[i] = Math.random() * 2 - 1;

  function voice(t, peak, attack, decay, { pan = 0, out = master, reverbSend = 0, delaySend = 0 } = {}) {
    const gain = ctx.createGain();
    gain.gain.setValueAtTime(0.0001, t);
    gain.gain.exponentialRampToValueAtTime(Math.max(peak, 0.0002), t + attack);
    gain.gain.exponentialRampToValueAtTime(0.0001, t + attack + decay);
    const panner = ctx.createStereoPanner();
    panner.pan.value = pan;
    gain.connect(panner).connect(out);
    for (const [send, target] of [[reverbSend, reverb], [delaySend, delay]]) {
      if (send > 0) {
        const s = ctx.createGain();
        s.gain.value = send;
        panner.connect(s).connect(target);
      }
    }
    return gain;
  }

  function osc(type, freq, t, stop, detune = 0) {
    const o = ctx.createOscillator();
    o.type = type;
    o.frequency.setValueAtTime(freq, t);
    o.detune.value = detune;
    o.start(t);
    o.stop(stop);
    return o;
  }

  function noiseBurst(t, stop, type, freq, q = 1) {
    const src = ctx.createBufferSource();
    src.buffer = noise;
    src.loop = true;
    src.loopStart = Math.random() * 0.5;
    const filter = ctx.createBiquadFilter();
    filter.type = type;
    filter.frequency.value = freq;
    filter.Q.value = q;
    src.connect(filter);
    src.start(t, src.loopStart);
    src.stop(stop);
    return filter;
  }

  function kick(t, v) {
    const end = t + 0.5;
    const body = osc("sine", 165, t, end);
    body.frequency.exponentialRampToValueAtTime(48, t + 0.09);
    body.frequency.exponentialRampToValueAtTime(40, t + 0.4);
    body.connect(voice(t, v * 0.95, 0.002, 0.34));
    // short click so the kick reads on small speakers
    noiseBurst(t, t + 0.03, "highpass", 2500).connect(voice(t, v * 0.18, 0.001, 0.015));
    bassBus.gain.cancelScheduledValues(t);
    bassBus.gain.setValueAtTime(0.75, t);
    bassBus.gain.linearRampToValueAtTime(1, t + 0.1);
  }

  function snare(t, v) {
    const end = t + 0.3;
    noiseBurst(t, end, "bandpass", 3200, 0.7).connect(voice(t, v * 0.5, 0.001, 0.16, { reverbSend: 0.25 }));
    const tone = osc("triangle", 215, t, end);
    tone.frequency.exponentialRampToValueAtTime(170, t + 0.07);
    tone.connect(voice(t, v * 0.35, 0.001, 0.09));
  }

  function hat(t, v) {
    const open = v > 0.5;
    noiseBurst(t, t + 0.25, "highpass", 8200)
      .connect(voice(t, v * 0.22, 0.001, open ? 0.09 : 0.035, { pan: 0.3 }));
  }

  /** FM pair: a sine carrier whose pitch is modulated by another sine. */
  function fm(freq, ratio, index, indexDecay, t, stop) {
    const carrier = osc("sine", freq, t, stop);
    const modulator = osc("sine", freq * ratio, t, stop);
    const amount = ctx.createGain();
    amount.gain.setValueAtTime(freq * index, t);
    amount.gain.exponentialRampToValueAtTime(Math.max(freq * index * 0.05, 0.01), t + indexDecay);
    modulator.connect(amount).connect(carrier.frequency);
    return carrier;
  }

  // Plucked string bass: a round fundamental, a little string growl that fades
  // fast, and a finger thump at the start. Notes ring past their length.
  function bass(t, v, midi, len) {
    const ring = Math.min(1.6, Math.max(0.35, len * 1.6));
    const end = t + ring + 0.1;
    const freq = hz(midi);
    const tone = ctx.createBiquadFilter();
    tone.type = "lowpass";
    tone.Q.value = 0.8;
    tone.frequency.setValueAtTime(1400 + 1200 * v, t);
    tone.frequency.exponentialRampToValueAtTime(320, t + 0.18);
    osc("triangle", freq, t, end).connect(tone);
    fm(freq, 2, 1.2 * v, 0.12, t, end).connect(tone);
    tone.connect(voice(t, v * 0.55, 0.004, ring, { out: bassBus, reverbSend: 0.05 }));
    noiseBurst(t, t + 0.04, "bandpass", 700, 1.2).connect(voice(t, v * 0.1, 0.001, 0.025, { out: bassBus }));
  }

  // Synth lead: three detuned saws over a square sub, a resonant filter that
  // opens on the attack, a glide from the previous note and vibrato on held notes.
  let lastLeadFreq = null;
  function lead(t, v, midi, len) {
    const dur = Math.max(0.1, len * 0.95);
    const end = t + dur + 0.3;
    const freq = hz(midi);
    const from = lastLeadFreq ?? freq;
    lastLeadFreq = freq;

    const filter = ctx.createBiquadFilter();
    filter.type = "lowpass";
    filter.Q.value = 7;
    filter.frequency.setValueAtTime(700, t);
    filter.frequency.exponentialRampToValueAtTime(1800 + 3200 * v, t + 0.03);
    filter.frequency.exponentialRampToValueAtTime(900, t + Math.max(dur, 0.12));

    const vibrato = osc("sine", 5.8, t, end);
    const depth = ctx.createGain();
    depth.gain.setValueAtTime(0, t);
    depth.gain.setValueAtTime(0, t + Math.min(dur, 0.18));
    depth.gain.linearRampToValueAtTime(dur > 0.3 ? 9 : 0, t + Math.min(dur, 0.45));
    vibrato.connect(depth);

    const mix = ctx.createGain();
    mix.gain.value = 0.3;
    mix.connect(filter);
    for (const [type, detune, level] of [["sawtooth", -12, 1], ["sawtooth", 0, 1], ["sawtooth", 12, 1], ["square", -1200, 0.5]]) {
      const o = osc(type, from, t, end, detune);
      o.frequency.exponentialRampToValueAtTime(freq, t + 0.035);
      depth.connect(o.detune);
      const g = ctx.createGain();
      g.gain.value = level;
      o.connect(g).connect(mix);
    }
    filter.connect(voice(t, v * 0.52, 0.006, dur + 0.12, { pan: -0.15, reverbSend: 0.3, delaySend: 0.3 }));
  }

  return {
    /** `step` is the length of a sixteenth note in seconds. */
    playNote(n, t, step) {
      const v = n.vel / 127;
      if (n.inst === "kick") kick(t, v);
      else if (n.inst === "snare") snare(t, v);
      else if (n.inst === "hat") hat(t, v);
      else if (n.inst === "bass") bass(t, v, n.midi, n.len * step);
      else if (n.inst === "lead") lead(t, v, n.midi, n.len * step);
    },
    /** Dotted-eighth echo that follows the tempo. */
    setTempo(bpm) {
      delay.delayTime.setTargetAtTime((60 / bpm) * 0.75, ctx.currentTime, 0.05);
    },
    setVolume(value) {
      volume.gain.setTargetAtTime(value, ctx.currentTime, 0.02);
    },
  };
}
