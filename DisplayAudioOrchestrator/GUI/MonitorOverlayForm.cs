using System;
using System.Collections.Generic;
using System.Drawing;
using System.Threading;
using System.Windows.Forms;
using DisplayAudioOrchestrator.CCD;
using DisplayAudioOrchestrator.Orchestrator;

// ============================================================
// MonitorOverlayForm — fullscreen overlay on each active monitor.
// Port of Show-MonitorIdentifyOverlays from PS1.
// Shows GDI short name (DISPLAY1), nickname if registered, resolution.
// Dismisses on click, key, or after 5 seconds.
// ============================================================

namespace DisplayAudioOrchestrator.GUI
{
    internal sealed class SingleMonitorOverlay : Form
    {
        private Label _lblNum, _lblName, _lblSub, _lblHint;

        public SingleMonitorOverlay(Screen screen, string dispNum, string label, string sub)
        {
            FormBorderStyle = FormBorderStyle.None;
            TopMost         = true;
            BackColor       = Color.Black;
            Opacity         = 0.82;
            StartPosition   = FormStartPosition.Manual;
            Bounds          = screen.Bounds;
            KeyPreview      = true;
            ShowInTaskbar   = false;

            _lblNum = new Label
            {
                Text      = dispNum,
                Font      = new Font("Segoe UI", 110f, FontStyle.Bold),
                ForeColor = Color.White,
                BackColor = Color.Transparent,
                AutoSize  = true
            };
            _lblName = new Label
            {
                Text      = label,
                Font      = new Font("Segoe UI", 28f, FontStyle.Bold),
                ForeColor = Color.White,
                BackColor = Color.Transparent,
                AutoSize  = true
            };
            _lblSub = new Label
            {
                Text      = sub,
                Font      = new Font("Segoe UI", 13f),
                ForeColor = Color.FromArgb(200, 200, 200),
                BackColor = Color.Transparent,
                AutoSize  = true
            };
            _lblHint = new Label
            {
                Text      = "Click or press any key to dismiss",
                Font      = new Font("Segoe UI", 11f),
                ForeColor = Color.FromArgb(110, 110, 110),
                BackColor = Color.Transparent,
                AutoSize  = true
            };

            Controls.AddRange(new Control[] { _lblNum, _lblName, _lblSub, _lblHint });
        }

        protected override void OnLoad(EventArgs e)
        {
            base.OnLoad(e);
            int w = ClientSize.Width, h = ClientSize.Height;
            _lblNum.Left  = (w - _lblNum.Width)  / 2;
            _lblNum.Top   = h / 2 - _lblNum.Height - 14;
            _lblName.Left = (w - _lblName.Width)  / 2;
            _lblName.Top  = _lblNum.Bottom + 14;
            _lblSub.Left  = (w - _lblSub.Width)   / 2;
            _lblSub.Top   = _lblName.Bottom + 10;
            _lblHint.Left = (w - _lblHint.Width)  / 2;
            _lblHint.Top  = h - 52;
        }

        protected override void OnClick(EventArgs e)   { base.OnClick(e);   Close(); }
        protected override void OnKeyDown(KeyEventArgs e) { base.OnKeyDown(e); Close(); }
    }

    public static class MonitorOverlayForm
    {
        public static void ShowOverlays()
        {
            var displays = DisplayConfigManager.GetAllDisplayInfo();
            var state    = StateStore.Load();
            var screens  = Screen.AllScreens;

            var overlays = new List<SingleMonitorOverlay>();

            foreach (var d in displays)
            {
                if (!d.Active) continue;

                Screen screen = null;
                foreach (var s in screens)
                {
                    string sName = s.DeviceName.TrimStart('\\', '.', '\\');
                    if (s.DeviceName.Equals(d.GdiDeviceName, StringComparison.OrdinalIgnoreCase) ||
                        s.DeviceName.EndsWith(d.GdiShortName, StringComparison.OrdinalIgnoreCase))
                    {
                        screen = s;
                        break;
                    }
                }
                if (screen == null) continue;

                string dispNum = d.GdiShortName.Replace("DISPLAY", string.Empty);

                // Find nickname
                string label = d.GdiShortName;
                foreach (var kv in state.Displays)
                {
                    var reg = kv.Value;
                    if (d.GdiShortName.Equals(reg.GdiName, StringComparison.OrdinalIgnoreCase) ||
                        (!string.IsNullOrEmpty(reg.FriendlyName) && d.FriendlyName != null &&
                         d.FriendlyName.IndexOf(reg.FriendlyName, StringComparison.OrdinalIgnoreCase) >= 0))
                    {
                        label = kv.Key + " — " + (d.FriendlyName ?? d.GdiShortName);
                        break;
                    }
                }
                if (label == d.GdiShortName && d.FriendlyName != null)
                    label = d.FriendlyName;

                string sub = $"{d.GdiDeviceName}  |  {d.Width}x{d.Height}@{d.Hz}Hz";

                overlays.Add(new SingleMonitorOverlay(screen, dispNum, label, sub));
            }

            if (overlays.Count == 0) return;

            var timer = new System.Windows.Forms.Timer { Interval = 5000 };
            timer.Tick += (s, e) =>
            {
                timer.Stop();
                foreach (var f in overlays)
                    if (!f.IsDisposed) f.Close();
            };

            foreach (var f in overlays) f.Show();
            timer.Start();

            while (true)
            {
                bool anyVisible = false;
                foreach (var f in overlays)
                    if (!f.IsDisposed && f.Visible) { anyVisible = true; break; }
                if (!anyVisible) break;
                Application.DoEvents();
                Thread.Sleep(16);
            }
            timer.Stop();
            foreach (var f in overlays) f.Dispose();
        }
    }
}
