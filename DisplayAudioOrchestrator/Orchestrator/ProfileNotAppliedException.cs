using System;

namespace DisplayAudioOrchestrator.Orchestrator
{
    public sealed class ProfileNotAppliedException : Exception
    {
        public string ProfileName { get; }
        public string Reason      { get; }

        public ProfileNotAppliedException(string profileName, string reason)
            : base($"Profile '{profileName}' was not applied: {reason}")
        {
            ProfileName = profileName;
            Reason      = reason;
        }
    }
}
