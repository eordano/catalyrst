export type AudioPreferences = { input: string; output: string; volume: number; incomingCalls: boolean };
let transient: AudioPreferences | undefined;
const defaults: AudioPreferences = {input:'default',output:'default',volume:100,incomingCalls:true};
export function audioPreferences(): AudioPreferences {
  if(transient)return {...transient};
  try { const value = JSON.parse(localStorage.getItem('dcl.social.audio') || '{}'); return {input:typeof value.input === 'string' ? value.input : 'default',output:typeof value.output === 'string' ? value.output : 'default',volume:typeof value.volume === 'number' ? Math.max(0,Math.min(100,value.volume)) : 100,incomingCalls:value.incomingCalls !== false}; } catch { return {...defaults}; }
}
export function saveAudioPreferences(patch: Partial<AudioPreferences>) {
  const value = {...audioPreferences(),...patch};
  try { localStorage.setItem('dcl.social.audio',JSON.stringify(value));transient=undefined; } catch { transient=value; }
  window.dispatchEvent(new CustomEvent('social:audio-preferences',{detail:value}));
}
