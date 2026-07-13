using System;

// ============================================================
// OrchestratorLogger — static logger.
// ALL levels always go to Console (verbose).
// GUI subscribes to LogEvent to receive WARN+ERROR for the output box.
// Debug messages are suppressed unless DebugMode is true.
// ============================================================

namespace DisplayAudioOrchestrator
{
    public enum LogLevel
    {
        Debug,
        Info,
        Warn,
        Error
    }

    public static class OrchestratorLogger
    {
        public static bool DebugMode { get; set; } = false;

        // GUI subscribes here to receive log messages for the output box.
        // Contract: handler is called on the same thread that called Log().
        // GUI must use Invoke() if it updates UI controls from a background thread.
        public static event Action<string, LogLevel> LogEvent;

        public static void Log(string message, LogLevel level = LogLevel.Info)
        {
            string prefix = LevelPrefix(level);
            string line   = $"{prefix}{message}";

            // Always dump to console
            Console.WriteLine(line);

            // Fire event so GUI can subscribe
            LogEvent?.Invoke(line, level);
        }

        public static void Debug(string message)
        {
            if (!DebugMode) return;
            Log(message, LogLevel.Debug);
        }

        private static string LevelPrefix(LogLevel level)
        {
            switch (level)
            {
                case LogLevel.Debug: return "[DBG]  ";
                case LogLevel.Info:  return "[INFO] ";
                case LogLevel.Warn:  return "[WARN] ";
                case LogLevel.Error: return "[ERR]  ";
                default:             return "[LOG]  ";
            }
        }
    }
}
