; NSIS installer hooks for TurboGet Native Messaging Host registration

!macro NSIS_HOOK_POSTINSTALL
  ; Register com.turboget.host manifest path in HKCU Registry for Chrome, Edge, and Brave
  WriteRegStr HKCU "Software\Google\Chrome\NativeMessagingHosts\com.turboget.host" "" "$INSTDIR\com.turboget.host.json"
  WriteRegStr HKCU "Software\Microsoft\Edge\NativeMessagingHosts\com.turboget.host" "" "$INSTDIR\com.turboget.host.json"
  WriteRegStr HKCU "Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.turboget.host" "" "$INSTDIR\com.turboget.host.json"

  ; Register magnet: URL protocol handler for TurboGet
  WriteRegStr HKCU "Software\Classes\magnet" "" "URL:Magnet Link"
  WriteRegStr HKCU "Software\Classes\magnet" "URL Protocol" ""
  WriteRegStr HKCU "Software\Classes\magnet\shell\open\command" "" '"$INSTDIR\turboget.exe" "%1"'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Remove Native Messaging host registry keys upon uninstall
  DeleteRegKey HKCU "Software\Google\Chrome\NativeMessagingHosts\com.turboget.host"
  DeleteRegKey HKCU "Software\Microsoft\Edge\NativeMessagingHosts\com.turboget.host"
  DeleteRegKey HKCU "Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.turboget.host"
  DeleteRegKey HKCU "Software\Classes\magnet"
!macroend
