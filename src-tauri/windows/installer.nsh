; v0.28.63 — bundle the Microsoft Visual C++ 2015-2022 runtime DLLs
; alongside app.exe so fresh Windows 11 installs don't hit
; "MSVCP140.dll was not found." Windows' DLL search order looks in
; the executable's directory before system dirs and PATH, so placing
; msvcp140/vcruntime140/vcruntime140_1 next to Travis.exe means the
; app finds them without the user having to install vc_redist.
;
; Tauri deploys the DLLs to $INSTDIR\resources\vc\ (per bundle.resources
; in tauri.windows.conf.json). POSTINSTALL copies them up to $INSTDIR
; where Windows will find them. POSTUNINSTALL removes them.
;
; Static-linking the CRT was tried first (v0.28.62) but failed at link
; time because fastembed pulls in ort/ort-sys, which ships pre-built
; ONNX Runtime binaries linked with /MD. Can't mix /MT (whisper.cpp
; forced to static) and /MD (ort prebuilt) in one binary. Shipping
; the DLLs is the fallback that doesn't require rebuilding ORT from
; source and avoids a UAC prompt at install time.

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Installing Visual C++ runtime DLLs..."
  CopyFiles /SILENT "$INSTDIR\resources\vc\msvcp140.dll" "$INSTDIR"
  CopyFiles /SILENT "$INSTDIR\resources\vc\vcruntime140.dll" "$INSTDIR"
  CopyFiles /SILENT "$INSTDIR\resources\vc\vcruntime140_1.dll" "$INSTDIR"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  Delete "$INSTDIR\msvcp140.dll"
  Delete "$INSTDIR\vcruntime140.dll"
  Delete "$INSTDIR\vcruntime140_1.dll"
!macroend
