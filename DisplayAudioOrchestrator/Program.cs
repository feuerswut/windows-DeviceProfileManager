using System;
using System.Windows.Forms;
using DisplayAudioOrchestrator.CCD;
using DisplayAudioOrchestrator.GUI;

// ============================================================
// Program — entry point.
// OutputType=Exe so the binary attaches to the calling console
// and CLI output is printed correctly.
// In GUI mode, the console window is hidden via ShowWindow.
// ============================================================

namespace DisplayAudioOrchestrator
{
    internal static class Program
    {
        [STAThread]
        static void Main(string[] args)
        {
            OrchestratorOptions opts;
            try
            {
                opts = OrchestratorCommandLineParser.Parse(args);
            }
            catch (ArgumentException ex)
            {
                Console.Error.WriteLine($"[ERR]  {ex.Message}");
                OrchestratorCommandLineParser.PrintHelp();
                Environment.Exit(1);
                return;
            }

            if (opts.Debug)
                OrchestratorLogger.DebugMode = true;

            if (opts.ShowGui || (args == null || args.Length == 0))
            {
                HideConsole();
                Application.EnableVisualStyles();
                Application.SetCompatibleTextRenderingDefault(false);
                Application.Run(new ProfileSwitcherForm());
                return;
            }

            OrchestratorProcessor.Process(opts);
        }

        private static void HideConsole()
        {
            IntPtr hWnd = DisplayConfigNativeMethods.GetConsoleWindow();
            if (hWnd != IntPtr.Zero)
                DisplayConfigNativeMethods.ShowWindow(hWnd, DisplayConfigNativeMethods.SW_HIDE);
        }
    }
}
