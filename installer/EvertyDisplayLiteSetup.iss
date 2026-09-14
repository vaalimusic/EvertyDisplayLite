#ifndef MyAppVersion
  #define MyAppVersion "1.3.0"
#endif

#define ProductName "EvertyDisplay Lite"
#define SetupBaseName "EvertyDisplay-Lite-Setup"
#define PayloadRoot "..\artifacts\installer-payload"

[Setup]
AppName={#ProductName}
AppId=EvertyDisplay
AppVersion={#MyAppVersion}
VersionInfoVersion={#MyAppVersion}.0
AppPublisher=Артур Валиев (Arthur Valiev)
AppPublisherURL=https://desk.everty.ru/evertydisplay
AppSupportURL=https://desk.everty.ru/evertydisplay
DefaultDirName={autopf}\EvertyDisplay
DefaultGroupName=EvertyDisplay
AllowNoIcons=yes
OutputDir=..\dist
OutputBaseFilename={#SetupBaseName}-{#MyAppVersion}
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=admin
CloseApplications=force
CloseApplicationsFilter=EvertyDisplay.exe,multitor-service.exe
RestartApplications=no
RestartIfNeededByRun=no
UsePreviousAppDir=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayIcon={app}\EvertyDisplay.exe
SetupIconFile=..\crates\ui\evertydisplay.ico
LicenseFile=..\LICENSE
SetupLogging=yes

[Languages]
Name: russian; MessagesFile: compiler:Languages\Russian.isl
Name: english; MessagesFile: compiler:Default.isl
Name: arabic; MessagesFile: compiler:Languages\Arabic.isl
Name: spanish; MessagesFile: compiler:Languages\Spanish.isl
Name: german; MessagesFile: compiler:Languages\German.isl
Name: french; MessagesFile: compiler:Languages\French.isl

[CustomMessages]
russian.AutostartTask=Запускать EvertyDisplay вместе с Windows
english.AutostartTask=Start EvertyDisplay with Windows
arabic.AutostartTask=تشغيل EvertyDisplay مع Windows
spanish.AutostartTask=Iniciar EvertyDisplay con Windows
german.AutostartTask=EvertyDisplay mit Windows starten
french.AutostartTask=Démarrer EvertyDisplay avec Windows
russian.StartupOptions=Параметры запуска:
english.StartupOptions=Startup options:
arabic.StartupOptions=خيارات التشغيل:
spanish.StartupOptions=Opciones de inicio:
german.StartupOptions=Startoptionen:
french.StartupOptions=Options de démarrage :
russian.InstallDriver=Установка и проверка видеоадаптера виртуальных мониторов...
english.InstallDriver=Installing and verifying the virtual display adapter...
arabic.InstallDriver=جارٍ تثبيت محول الشاشة الافتراضية والتحقق منه...
spanish.InstallDriver=Instalando y comprobando el adaptador de pantalla virtual...
german.InstallDriver=Der virtuelle Bildschirmadapter wird installiert und überprüft...
french.InstallDriver=Installation et vérification de l’adaptateur d’écran virtuel...
russian.RemoveDriver=Безопасное удаление видеодрайвера EvertyDisplay...
english.RemoveDriver=Safely removing the EvertyDisplay display driver...
arabic.RemoveDriver=جارٍ إزالة برنامج تشغيل EvertyDisplay بأمان...
spanish.RemoveDriver=Eliminando de forma segura el controlador de EvertyDisplay...
german.RemoveDriver=Der EvertyDisplay-Bildschirmtreiber wird sicher entfernt...
french.RemoveDriver=Suppression sécurisée du pilote d’affichage EvertyDisplay...
russian.Arm64Unsupported=Эта сборка содержит драйвер x64 и не может быть установлена на Windows ARM64.
english.Arm64Unsupported=This build contains an x64 driver and cannot be installed on Windows ARM64.
arabic.Arm64Unsupported=يحتوي هذا الإصدار على برنامج تشغيل x64 ولا يمكن تثبيته على Windows ARM64.
spanish.Arm64Unsupported=Esta versión incluye un controlador x64 y no se puede instalar en Windows ARM64.
german.Arm64Unsupported=Diese Version enthält einen x64-Treiber und kann nicht unter Windows ARM64 installiert werden.
french.Arm64Unsupported=Cette version contient un pilote x64 et ne peut pas être installée sur Windows ARM64.

[Tasks]
Name: desktopicon; Description: {cm:CreateDesktopIcon}; GroupDescription: {cm:AdditionalIcons}
Name: autostart; Description: {cm:AutostartTask}; GroupDescription: {cm:StartupOptions}

[Files]
Source: {#PayloadRoot}\EvertyDisplay.exe; DestDir: {app}; Flags: ignoreversion
Source: {#PayloadRoot}\multitor-service.exe; DestDir: {app}; Flags: ignoreversion
Source: {#PayloadRoot}\evertydisplay.ico; DestDir: {app}; Flags: ignoreversion
Source: {#PayloadRoot}\driver\signed_x64\*; DestDir: {app}\driver\signed_x64; Flags: ignoreversion recursesubdirs createallsubdirs
Source: ..\LICENSE; DestDir: {app}; Flags: ignoreversion
Source: ..\NOTICE; DestDir: {app}; Flags: ignoreversion
Source: ..\THIRD_PARTY_LICENSES.tsv; DestDir: {app}; Flags: ignoreversion
Source: ..\third-party\virtual-display-driver\LICENSE; DestDir: {app}\driver; DestName: VIRTUAL_DISPLAY_DRIVER_LICENSE.txt; Flags: ignoreversion
Source: ..\third-party\virtual-display-driver\NOTICE.md; DestDir: {app}\driver; DestName: VIRTUAL_DISPLAY_DRIVER_NOTICE.md; Flags: ignoreversion

[Icons]
Name: {group}\EvertyDisplay; Filename: {app}\EvertyDisplay.exe; IconFilename: {app}\evertydisplay.ico
Name: {group}\{cm:UninstallProgram,EvertyDisplay}; Filename: {uninstallexe}
Name: {autodesktop}\EvertyDisplay; Filename: {app}\EvertyDisplay.exe; IconFilename: {app}\evertydisplay.ico; Tasks: desktopicon

[Run]
Filename: {app}\EvertyDisplay.exe; Parameters: --install-driver; StatusMsg: {cm:InstallDriver}; Flags: runhidden waituntilterminated
Filename: {app}\EvertyDisplay.exe; Parameters: --enable-autostart; Flags: runhidden waituntilterminated runasoriginaluser; Tasks: autostart
Filename: {app}\EvertyDisplay.exe; Description: {cm:LaunchProgram,EvertyDisplay}; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: taskkill.exe; Parameters: /F /IM EvertyDisplay.exe; Flags: runhidden; RunOnceId: StopUi
Filename: taskkill.exe; Parameters: /F /IM multitor-service.exe; Flags: runhidden; RunOnceId: StopService
Filename: {app}\EvertyDisplay.exe; Parameters: --disable-autostart; Flags: runhidden waituntilterminated; RunOnceId: DisableAutostart
Filename: {app}\EvertyDisplay.exe; Parameters: --uninstall-driver; StatusMsg: {cm:RemoveDriver}; Flags: runhidden waituntilterminated; RunOnceId: RemoveDriver

[Code]
function InitializeSetup(): Boolean;
begin
  if IsArm64 then
  begin
    MsgBox(ExpandConstant('{cm:Arm64Unsupported}'), mbError, MB_OK);
    Result := False;
  end
  else
    Result := True;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  ResultCode: Integer;
begin
  { Gracefully close the UI first, then force only processes that did not exit. }
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/IM EvertyDisplay.exe /T', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Sleep(500);
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM EvertyDisplay.exe /T', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM multitor-service.exe /T', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Result := '';
end;
