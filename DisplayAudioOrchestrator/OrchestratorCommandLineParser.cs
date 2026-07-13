using System;
using System.Collections.Generic;

namespace DisplayAudioOrchestrator
{
    public sealed class OrchestratorOptions
    {
        public bool   ShowGui        { get; set; } = false;
        public bool   ListProfiles   { get; set; } = false;
        public bool   ListDevices    { get; set; } = false;
        public bool   Identify       { get; set; } = false;
        public bool   Debug          { get; set; } = false;
        public string ApplyProfile   { get; set; }
        public string SaveProfile    { get; set; }
        public int?   SetVolumeAll   { get; set; }
        public bool   Help           { get; set; } = false;
    }

    public static class OrchestratorCommandLineParser
    {
        public static OrchestratorOptions Parse(string[] args)
        {
            var opts = new OrchestratorOptions();
            if (args == null || args.Length == 0)
            {
                opts.ShowGui = true;
                return opts;
            }

            for (int i = 0; i < args.Length; i++)
            {
                string a = args[i].ToLowerInvariant();
                switch (a)
                {
                    case "--gui":
                        opts.ShowGui = true;
                        break;
                    case "--list-profiles":
                        opts.ListProfiles = true;
                        break;
                    case "--list-devices":
                        opts.ListDevices = true;
                        break;
                    case "--identify":
                        opts.Identify = true;
                        break;
                    case "--debug":
                        opts.Debug = true;
                        break;
                    case "--help": case "-h": case "/?":
                        opts.Help = true;
                        break;
                    case "--apply-profile":
                        opts.ApplyProfile = NextArg(args, ref i, "--apply-profile");
                        break;
                    case "--save-profile":
                        opts.SaveProfile = NextArg(args, ref i, "--save-profile");
                        break;
                    case "--set-volume-all":
                    {
                        string val = NextArg(args, ref i, "--set-volume-all");
                        int v;
                        if (int.TryParse(val, out v) && v >= 0 && v <= 100)
                            opts.SetVolumeAll = v;
                        else
                            throw new ArgumentException($"--set-volume-all requires an integer 0-100, got: {val}");
                        break;
                    }
                    default:
                        OrchestratorLogger.Log($"Unknown argument: {args[i]}", LogLevel.Warn);
                        break;
                }
            }

            return opts;
        }

        private static string NextArg(string[] args, ref int i, string flag)
        {
            i++;
            if (i >= args.Length)
                throw new ArgumentException($"{flag} requires a value");
            return args[i];
        }

        public static void PrintHelp()
        {
            Console.WriteLine(@"DisplayAudioOrchestrator — display & audio profile manager

Usage:
  DisplayAudioOrchestrator.exe [options]

Options:
  (no args)                    Open profile switcher GUI
  --gui                        Open profile switcher GUI explicitly
  --apply-profile <name>       Apply a saved profile
  --save-profile  <name>       Save current state as a profile (opens wizard)
  --list-profiles              List all saved profiles
  --list-devices               List all displays and audio devices
  --identify                   Show monitor overlay with GDI names
  --set-volume-all <0-100>     Set volume on all active playback devices
  --debug                      Enable verbose debug output
  --help                       Show this help");
        }
    }
}
