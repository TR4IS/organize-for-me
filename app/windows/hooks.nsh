; Lets the browser extension ask the app where a download is now. Each Chromium-based browser
; looks the app up under its own key. Uninstalling removes only this app's own key, never a parent.
!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr HKCU "Software\Google\Chrome\NativeMessagingHosts\com.tr4is.organizeforme" "" "$INSTDIR\chrome-host.json"
  WriteRegStr HKCU "Software\Microsoft\Edge\NativeMessagingHosts\com.tr4is.organizeforme" "" "$INSTDIR\chrome-host.json"
  WriteRegStr HKCU "Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.tr4is.organizeforme" "" "$INSTDIR\chrome-host.json"
  WriteRegStr HKCU "Software\Chromium\NativeMessagingHosts\com.tr4is.organizeforme" "" "$INSTDIR\chrome-host.json"
  WriteRegStr HKCU "Software\Thorium\NativeMessagingHosts\com.tr4is.organizeforme" "" "$INSTDIR\chrome-host.json"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegKey HKCU "Software\Google\Chrome\NativeMessagingHosts\com.tr4is.organizeforme"
  DeleteRegKey HKCU "Software\Microsoft\Edge\NativeMessagingHosts\com.tr4is.organizeforme"
  DeleteRegKey HKCU "Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.tr4is.organizeforme"
  DeleteRegKey HKCU "Software\Chromium\NativeMessagingHosts\com.tr4is.organizeforme"
  DeleteRegKey HKCU "Software\Thorium\NativeMessagingHosts\com.tr4is.organizeforme"
!macroend
