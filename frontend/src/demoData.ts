export interface DemoScenario {
  id: string;
  eyebrow: string;
  title: string;
  prompt: string;
  summary: string;
  tone: string;
  status: string;
  palette: {
    glow: string;
    accent: string;
    wash: string;
  };
  tools: string[];
  timeline: Array<{
    phase: string;
    detail: string;
  }>;
  response: string;
  panels: Array<{
    title: string;
    subtitle: string;
    lines: string[];
  }>;
}

export const demoScenarios: DemoScenario[] = [
  {
    id: "spotify",
    eyebrow: "MEDIA CONTROL",
    title: "Echo plays the exact mood, not just the app.",
    prompt: "echo, play something warm and low-key on Spotify for late-night coding.",
    summary: "Echo launches Spotify, finds a fitting mix, and starts playback without asking for rigid commands.",
    tone: "Calm · Intent-aware · Hands-free",
    status: "Spotify playing: Late Night Focus",
    palette: {
      glow: "rgba(110, 255, 175, 0.24)",
      accent: "#7dff9e",
      wash: "rgba(33, 102, 58, 0.28)",
    },
    tools: ["speech_to_text", "planner", "macos_automation_tool", "desktop_tool"],
    timeline: [
      { phase: "Listen", detail: "Transcribes the spoken request and extracts mood + destination app." },
      { phase: "Reason", detail: "Infers Spotify is the best target and chooses a softer focus playlist." },
      { phase: "Act", detail: "Activates Spotify and issues playback commands through macOS automation." },
      { phase: "Verify", detail: "Confirms the player is active and audio controls are responsive." },
    ],
    response:
      "Playing a mellow Spotify mix for late-night coding now. I picked something warm and unobtrusive so it stays in the background.",
    panels: [
      {
        title: "Assistant Transcript",
        subtitle: "Voice-first reasoning",
        lines: [
          "User: play something warm and low-key on Spotify for late-night coding",
          "Echo: matching mood to destination app",
          "Echo: launching Spotify and starting playback",
        ],
      },
      {
        title: "Spotify",
        subtitle: "Player state",
        lines: [
          "Now Playing",
          "Late Night Focus",
          "crossfade on · volume 54%",
          "playback active",
        ],
      },
      {
        title: "Execution Trace",
        subtitle: "Autonomous tool chain",
        lines: [
          "activate Spotify",
          "play",
          "set playback context",
          "confirm active window",
        ],
      },
    ],
  },
  {
    id: "gmail",
    eyebrow: "EMAIL EXECUTION",
    title: "Echo drafts the message you meant to send.",
    prompt: "echo, open Gmail and email Vishay that I need to get out of tomorrow's meeting. Make it respectful.",
    summary:
      "Echo opens Gmail on the signed-in session, drafts a real note to Vishay at v.agrwl17@gmail.com, and keeps the tone polished.",
    tone: "Respectful · Clear · Human",
    status: "Draft ready to send in Gmail",
    palette: {
      glow: "rgba(255, 116, 116, 0.22)",
      accent: "#ff8f7a",
      wash: "rgba(110, 35, 35, 0.26)",
    },
    tools: ["speech_to_text", "planner", "browser_tool", "macos_ui_tool", "desktop_tool"],
    timeline: [
      { phase: "Listen", detail: "Extracts recipient, delivery channel, intent, and requested tone." },
      { phase: "Compose", detail: "Writes a concise, respectful email instead of dumping raw text." },
      { phase: "Navigate", detail: "Brings Gmail to the foreground and opens a compose flow." },
      { phase: "Populate", detail: "Fills Vishay, v.agrwl17@gmail.com, subject, and body in the signed-in session." },
    ],
    response:
      "I opened Gmail and drafted a respectful note to Vishay about tomorrow's meeting. It's ready in your signed-in inbox flow.",
    panels: [
      {
        title: "Prompt Understanding",
        subtitle: "Structured intent",
        lines: [
          "Recipient: Vishay",
          "Email: v.agrwl17@gmail.com",
          "Goal: get out of tomorrow's meeting",
          "Tone: respectful",
        ],
      },
      {
        title: "Draft Preview",
        subtitle: "Real generated email",
        lines: [
          "Subject: Quick note about tomorrow's meeting",
          "Hi Vishay,",
          "I wanted to reach out ahead of tomorrow's meeting.",
          "I need to step out of it and didn't want to leave you without notice.",
          "Thanks for understanding.",
        ],
      },
      {
        title: "Gmail Session",
        subtitle: "Browser control",
        lines: [
          "compose window open",
          "to: Vishay <v.agrwl17@gmail.com>",
          "draft autosaved",
          "ready for send",
        ],
      },
    ],
  },
  {
    id: "messages",
    eyebrow: "MESSAGING",
    title: "Echo sends the text in the right tone.",
    prompt: "echo, text Vishay on iMessage that I can't make it today, but make it polite.",
    summary:
      "Echo chooses Messages, writes a softer version of the text, and sends it through the native iMessage flow.",
    tone: "Polite · Fast · Native",
    status: "iMessage delivered to Vishay",
    palette: {
      glow: "rgba(107, 196, 255, 0.24)",
      accent: "#88d8ff",
      wash: "rgba(24, 58, 96, 0.28)",
    },
    tools: ["speech_to_text", "planner", "communications_tool", "macos_automation_tool"],
    timeline: [
      { phase: "Interpret", detail: "Maps 'text Vishay' to the iMessage path and preserves the polite tone request." },
      { phase: "Write", detail: "Generates a cleaner version of the message instead of forwarding the raw words." },
      { phase: "Send", detail: "Uses the native Messages route to deliver the text." },
      { phase: "Confirm", detail: "Reports successful handoff instead of stopping at message generation." },
    ],
    response:
      "I sent Vishay a polite iMessage saying you can't make it today and kept the tone warm instead of abrupt.",
    panels: [
      {
        title: "Generated Text",
        subtitle: "Polished automatically",
        lines: [
          "Hey Vishay,",
          "I wanted to let you know I won't be able to make it today.",
          "Sorry for the short notice, and thanks for understanding.",
        ],
      },
      {
        title: "Messages",
        subtitle: "Native app flow",
        lines: [
          "conversation: Vishay",
          "message inserted",
          "sent via iMessage",
          "delivery confirmed",
        ],
      },
      {
        title: "Why It Works",
        subtitle: "Agent behavior",
        lines: [
          "infers platform",
          "rewrites tone",
          "sends via correct channel",
          "verifies completion",
        ],
      },
    ],
  },
  {
    id: "finder",
    eyebrow: "FILE ORGANIZATION",
    title: "Echo cleans the mess, not just the folder window.",
    prompt: "echo, reorganize Vishay's files in Finder into labeled folders.",
    summary:
      "Echo reads the directory, groups related assets, creates labeled folders, and moves everything into a cleaner structure.",
    tone: "Systematic · Quiet · Reliable",
    status: "Finder workspace reorganized",
    palette: {
      glow: "rgba(249, 213, 118, 0.22)",
      accent: "#ffd36e",
      wash: "rgba(98, 72, 23, 0.28)",
    },
    tools: ["speech_to_text", "planner", "filesystem_tool", "app_control_tool", "macos_ui_tool"],
    timeline: [
      { phase: "Scan", detail: "Maps the file set and detects obvious groups like docs, media, code, and archives." },
      { phase: "Plan", detail: "Creates labeled target folders before moving anything destructive." },
      { phase: "Move", detail: "Organizes files into cleaner buckets while preserving names." },
      { phase: "Show", detail: "Brings Finder forward on the reorganized view so the result is visible immediately." },
    ],
    response:
      "I reorganized the folder into labeled sections so the loose files are grouped by what they are and easier to scan.",
    panels: [
      {
        title: "Before",
        subtitle: "Unsorted workspace",
        lines: [
          "invoice.pdf",
          "IMG_4410.PNG",
          "roadmap-notes.txt",
          "build.zip",
          "demo.mov",
        ],
      },
      {
        title: "After",
        subtitle: "Labeled structure",
        lines: [
          "/Documents",
          "/Images",
          "/Videos",
          "/Archives",
          "/Project Notes",
        ],
      },
      {
        title: "Execution",
        subtitle: "End-to-end",
        lines: [
          "mkdir targets",
          "classify files",
          "move items safely",
          "reveal final structure in Finder",
        ],
      },
    ],
  },
];
