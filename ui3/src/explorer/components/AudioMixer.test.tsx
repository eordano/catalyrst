import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import AudioMixer from "./AudioMixer";
import QuickAudio from "./QuickAudio";
import { AudioMixerProvider, readAudioSources, type AudioSource } from "../../overlay/audioMixer";

const sources: [AudioSource, AudioSource] = [
  { id: "0123456789abcdef", name: "Dance floor", source: "music.mp3", area: "Club", type: "Sound effects", playing: true, inScene: true, volume: 1, distance: 12.4 },
  { id: "fedcba9876543210", name: "Rooftop", source: "music.mp3", area: "Club", type: "Audio streams", playing: true, inScene: false, volume: 1 },
];
const storage = "dcl.audio.sources.v1";
let live: AudioSource[];
let command: ReturnType<typeof vi.fn<(text: string) => Promise<string>>>;
function mount(quick = false) {
  return render(<AudioMixerProvider>{quick ? <QuickAudio /> : <AudioMixer advanced />}</AudioMixerProvider>);
}
beforeEach(() => {
  localStorage.removeItem(storage);
  live = sources.map(s => ({ ...s }));
  command = vi.fn(async (text: string) => {
    const match = text.match(/--set (\w+) --volume ([\d.]+)/);
    if (match) live = live.map(s => s.id === match[1] ? { ...s, volume: Number(match[2]) } : s);
    return JSON.stringify({ sources: live });
  });
  window.engine_console_command = command;
});
afterEach(() => { cleanup(); delete window.engine_console_command; localStorage.removeItem(storage); vi.useRealTimers(); });

describe("individual sound controls", () => {
  it("groups sources, pins an item, changes only its gain, restores mute level, and removes its override", async () => {
    mount();
    expect(await screen.findByText("Scene volume \u00b7 12 m away")).toBeInTheDocument();
    fireEvent.click(await screen.findByRole("button", { name: "Add Dance floor control" }));
    fireEvent.change(screen.getByLabelText("Dance floor volume"), { target: { value: "35" } });
    fireEvent.keyUp(screen.getByLabelText("Dance floor volume"), { key: "ArrowLeft" });
    await waitFor(() => expect(readAudioSources()[sources[0].id]?.volume).toBe(.35));
    expect(command).toHaveBeenCalledWith(`/audio_sources --set ${sources[0].id} --volume 0.35`);
    expect(live[1]?.volume).toBe(1);
    fireEvent.click(screen.getByRole("button", { name: "Mute Dance floor" }));
    await waitFor(() => expect(readAudioSources()[sources[0].id]?.volume).toBe(0));
    fireEvent.click(screen.getByRole("button", { name: "Unmute Dance floor" }));
    await waitFor(() => expect(readAudioSources()[sources[0].id]?.volume).toBe(.35));
    fireEvent.change(screen.getByRole("combobox", { name: "Group by" }), { target: { value: "source" } });
    expect(screen.getByRole("heading", { name: "Music" })).toBeInTheDocument();
    fireEvent.change(screen.getByRole("combobox", { name: "Group by" }), { target: { value: "type" } });
    expect(screen.getByRole("heading", { name: "Audio streams" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Remove Dance floor control" }));
    await waitFor(() => expect(readAudioSources()[sources[0].id]).toBeUndefined());
    expect(live[0]?.volume).toBe(1);
    expect(screen.getByRole("button", { name: "Add Dance floor control" })).toBeInTheDocument();
  });

  it("restores preferences on engine connection and retains unavailable sources", async () => {
    localStorage.setItem(storage, JSON.stringify({ [sources[0].id]: { ...sources[0], volume: .2 } }));
    live = [];
    mount();
    await waitFor(() => expect(command).toHaveBeenCalledWith(`/audio_sources --set ${sources[0].id} --volume 0.2`));
    expect(screen.getByText("Currently unavailable \u00b7 preference saved")).toBeInTheDocument();
    expect(screen.getByLabelText("Dance floor volume")).toHaveValue("20");
  });

  it("keeps failed changes out of saved preferences and reports errors", async () => {
    mount();
    fireEvent.click(await screen.findByRole("button", { name: "Add Rooftop control" }));
    expect(screen.getByText("Outside your current scene \u00b7 silent")).toBeInTheDocument();
    command.mockRejectedValueOnce(new Error("Disconnected"));
    fireEvent.click(screen.getByRole("button", { name: "Mute Rooftop" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not change this source");
    expect(readAudioSources()[sources[1].id]?.volume).toBe(1);
  });

  it("shows one advanced switch and retains pinned controls in the quick view", async () => {
    mount(true);
    expect(screen.getAllByRole("button", { name: "Advanced sound controls" })).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: "Advanced sound controls" }));
    fireEvent.click(await screen.findByRole("button", { name: "Add Dance floor control" }));
    fireEvent.click(screen.getByRole("button", { name: "Basic sound controls" }));
    expect(screen.getByLabelText("Dance floor volume")).toBeInTheDocument();
    expect(screen.queryByRole("searchbox")).not.toBeInTheDocument();
  });

  it("times out stalled source requests and recovers on retry", async () => {
    vi.useFakeTimers();
    command.mockImplementationOnce(() => new Promise(() => {}));
    mount();
    const advanceUntil = async (done: () => boolean) => {
      for (let i = 0; i < 50 && !done(); i++) await act(async () => { await vi.advanceTimersToNextTimerAsync(); });
    };
    await advanceUntil(() => screen.queryByRole("alert") !== null);
    expect(screen.getByRole("alert")).toHaveTextContent("Could not refresh audio sources");
    await advanceUntil(() => screen.queryByRole("alert") === null);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add Dance floor control" })).toBeInTheDocument();
  });

  it("rejects corrupted persisted controls", () => {
    localStorage.setItem(storage, JSON.stringify({ [sources[0].id]: { ...sources[0], volume: 8 }, [sources[1].id]: { ...sources[1], previousVolume: -1 } }));
    expect(readAudioSources()).toEqual({});
  });
});
