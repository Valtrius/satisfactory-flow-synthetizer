# Benchmark-only Windows launch and topology helpers. Never changes host policy.
if (-not ('SolverBenchmarkProcess' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;

public sealed class SolverBenchmarkChild {
    public Process Process;
    public string RequestedMask;
    public string ObservedMask;
    public bool BeforeResume;
    public double AppliedSeconds;
}
public static class SolverBenchmarkProcess {
    [StructLayout(LayoutKind.Sequential)] struct Security {
        public int Length; public IntPtr Descriptor; public int Inherit;
    }
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] struct Startup {
        public int Size; public string Reserved, Desktop, Title;
        public int X, Y, XSize, YSize, XChars, YChars, Fill, Flags;
        public short Show, ReservedBytes; public IntPtr ReservedData, Input, Output, Error;
    }
    [StructLayout(LayoutKind.Sequential)] struct Info {
        public IntPtr Process, Thread; public int ProcessId, ThreadId;
    }
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern bool CreateProcess(string app, StringBuilder command, IntPtr pa, IntPtr ta,
        bool inherit, uint flags, IntPtr environment, string cwd, ref Startup start, out Info info);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern IntPtr CreateFile(string path, uint access, uint share, ref Security security,
        uint creation, uint flags, IntPtr template);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool SetProcessAffinityMask(IntPtr process, UIntPtr mask);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool GetProcessAffinityMask(IntPtr process, out UIntPtr mask, out UIntPtr system);
    [DllImport("kernel32.dll", SetLastError=true)] static extern uint ResumeThread(IntPtr thread);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool TerminateProcess(IntPtr process, uint code);
    [DllImport("kernel32.dll")] static extern uint WaitForSingleObject(IntPtr handle, uint millis);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool GetLogicalProcessorInformationEx(int relation, IntPtr buffer, ref uint length);

    static string Quote(string arg) {
        var value = new StringBuilder("\""); int slashes = 0;
        foreach (char c in arg) {
            if (c == '\\') { slashes++; continue; }
            if (c == '"') { value.Append('\\', slashes * 2 + 1); value.Append(c); }
            else { value.Append('\\', slashes); value.Append(c); }
            slashes = 0;
        }
        value.Append('\\', slashes * 2); value.Append('"'); return value.ToString();
    }
    static IntPtr Open(string path, bool input) {
        var security = new Security {Length=Marshal.SizeOf(typeof(Security)), Inherit=1};
        var handle = CreateFile(path, input ? 0x80000000u : 0x40000000u, 3, ref security,
            input ? 3u : 2u, 0x80, IntPtr.Zero);
        if (handle == new IntPtr(-1)) throw new Win32Exception(Marshal.GetLastWin32Error());
        return handle;
    }
    public static SolverBenchmarkChild Start(string exe, string[] args, string cwd,
        string stdout, string stderr, string requested) {
        var timer = Stopwatch.StartNew(); var handles = new List<IntPtr>();
        var info = new Info(); Process process = null; bool resumed = false;
        try {
            var command = new StringBuilder(Quote(exe));
            foreach (var arg in args) command.Append(" ").Append(Quote(arg));
            var input = Open("NUL", true); handles.Add(input);
            var output = Open(stdout, false); handles.Add(output);
            var error = Open(stderr, false); handles.Add(error);
            var start = new Startup {Size=Marshal.SizeOf(typeof(Startup)), Flags=0x100,
                Input=input, Output=output, Error=error};
            if (!CreateProcess(exe, command, IntPtr.Zero, IntPtr.Zero, true, 0x08000004,
                IntPtr.Zero, cwd, ref start, out info)) throw new Win32Exception(Marshal.GetLastWin32Error());
            // The primary thread is suspended. No solver setup or worker has run.
            if (!String.IsNullOrEmpty(requested) && !SetProcessAffinityMask(info.Process,
                new UIntPtr(Convert.ToUInt64(requested, 16)))) throw new Win32Exception(Marshal.GetLastWin32Error());
            UIntPtr observed, system;
            if (!GetProcessAffinityMask(info.Process, out observed, out system)) throw new Win32Exception(Marshal.GetLastWin32Error());
            var mask = observed.ToUInt64().ToString("x");
            if (!String.IsNullOrEmpty(requested) && mask != requested) throw new InvalidOperationException("Affinity mismatch before resume");
            process = Process.GetProcessById(info.ProcessId);
            var retainedHandle = process.Handle; // Retain exit status even for an immediately exiting child.
            double applied = timer.Elapsed.TotalSeconds;
            if (ResumeThread(info.Thread) != 1) throw new Win32Exception(Marshal.GetLastWin32Error());
            resumed = true;
            return new SolverBenchmarkChild {Process=process, RequestedMask=requested,
                ObservedMask=mask, BeforeResume=true, AppliedSeconds=applied};
        } finally {
            if (!resumed && info.Process != IntPtr.Zero) {
                TerminateProcess(info.Process, 1); WaitForSingleObject(info.Process, 10000);
                if (process != null) process.Dispose();
            }
            if (info.Thread != IntPtr.Zero) CloseHandle(info.Thread);
            if (info.Process != IntPtr.Zero) CloseHandle(info.Process);
            foreach (var handle in handles) CloseHandle(handle);
        }
    }
    public static object[] Topology() {
        var result = new List<object>();
        foreach (int relation in new int[] {0, 2}) {
            uint length = 0;
            GetLogicalProcessorInformationEx(relation, IntPtr.Zero, ref length);
            if (length == 0) throw new Win32Exception(Marshal.GetLastWin32Error());
            IntPtr buffer = Marshal.AllocHGlobal((int)length);
            try {
                if (!GetLogicalProcessorInformationEx(relation, buffer, ref length)) throw new Win32Exception(Marshal.GetLastWin32Error());
                for (int offset=0; offset < length;) {
                    IntPtr item = IntPtr.Add(buffer, offset);
                    int size = Marshal.ReadInt32(item, 4);
                    if (size < 8 || offset + size > length) throw new InvalidOperationException("Invalid topology size");
                    int count = (ushort)Marshal.ReadInt16(item, relation == 0 ? 30 : 38);
                    if (count == 0) count = 1;
                    int masksOffset = relation == 0 ? 32 : 40;
                    var groups = new List<object>();
                    for (int i=0; i<count; i++) {
                        int at = masksOffset + i * 16;
                        if (at + 16 > size) throw new InvalidOperationException("Invalid topology masks");
                        groups.Add(new {group=(ushort)Marshal.ReadInt16(item, at+8),
                            mask=unchecked((ulong)Marshal.ReadInt64(item, at)).ToString("x")});
                    }
                    result.Add(new {relation=relation, level=relation == 2 ? Marshal.ReadByte(item, 8) : 0,
                        cache_bytes=relation == 2 ? (uint)Marshal.ReadInt32(item, 12) : 0,
                        efficiency_class=relation == 0 ? Marshal.ReadByte(item, 9) : 0,
                        groups=groups.ToArray()});
                    offset += size;
                }
            } finally { Marshal.FreeHGlobal(buffer); }
        }
        return result.ToArray();
    }
}
'@
}

function ConvertTo-BenchmarkAffinityMask([string]$Value) {
    if (-not $Value) { return '' }
    if ($Value -notmatch '^[0-9a-fA-F]{1,16}$') { throw 'Affinity must be a hexadecimal mask string' }
    $mask = [Convert]::ToUInt64($Value, 16)
    if ($mask -eq 0 -or $mask -gt [long]::MaxValue) { throw 'Affinity must be nonzero and fit signed IntPtr' }
    $available = [Diagnostics.Process]::GetCurrentProcess().ProcessorAffinity.ToInt64()
    if (($mask -band $available) -ne $mask) { throw 'Affinity is outside the available process mask' }
    return $mask.ToString('x')
}

function Start-BenchmarkProcess {
    param([string]$Executable, [string[]]$Arguments, [string]$WorkingDirectory,
        [string]$StandardOutput, [string]$StandardError, [string]$Affinity = '')
    $mask = ConvertTo-BenchmarkAffinityMask $Affinity
    [SolverBenchmarkProcess]::Start($Executable, $Arguments, $WorkingDirectory,
        $StandardOutput, $StandardError, $mask)
}

function Save-BenchmarkTopology([string]$Path) {
    [ordered]@{
        schema_version = 1
        available_mask = [Diagnostics.Process]::GetCurrentProcess().ProcessorAffinity.ToInt64().ToString('x')
        relationships = [SolverBenchmarkProcess]::Topology()
        power_scheme = ((& powercfg.exe /getactivescheme) -join ' ')
        recorded_at = (Get-Date -Format o)
    } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $Path
}
