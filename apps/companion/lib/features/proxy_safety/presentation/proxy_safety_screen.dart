import 'package:flutter/material.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

import '../../../shared/brand/app_tester_mark.dart';
import 'proxy_safety_view_model.dart';

class ProxySafetyScreen extends StatefulWidget {
  const ProxySafetyScreen({required this.viewModel, super.key});

  final ProxySafetyViewModel viewModel;

  @override
  State<ProxySafetyScreen> createState() => _ProxySafetyScreenState();
}

class _ProxySafetyScreenState extends State<ProxySafetyScreen> {
  final _scanner = MobileScannerController(autoStart: false);
  bool _scanning = false;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _startScanner());
  }

  Future<void> _startScanner() async {
    if (_scanning || widget.viewModel.status?.isVpnActive == true) return;
    setState(() => _scanning = true);
    await _scanner.start();
  }

  Future<void> _onDetect(BarcodeCapture capture) async {
    final payload = capture.barcodes.firstOrNull?.rawValue;
    if (payload == null || widget.viewModel.isWorking) return;
    await _scanner.stop();
    if (mounted) setState(() => _scanning = false);
    await widget.viewModel.connectFromQr(payload);
    if (mounted &&
        widget.viewModel.status?.isVpnActive != true &&
        widget.viewModel.status?.isMonitoring != true &&
        widget.viewModel.status?.targetPackage == null) {
      await _startScanner();
    }
  }

  @override
  void dispose() {
    _scanner.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
        animation: widget.viewModel,
        builder: (context, _) {
          final model = widget.viewModel;
          final active = model.status?.isVpnActive == true;
          final connected = active || model.status?.isMonitoring == true;
          return Scaffold(
            body: SafeArea(
              child: LayoutBuilder(builder: (context, constraints) {
                final wide = constraints.maxWidth >= 720;
                final scanPanel = _ScanPanel(active: connected, scanning: _scanning, scanner: _scanner, onDetect: _onDetect);
                final connectionPanel = _ConnectionPanel(model: model, active: active, connected: connected, onDisconnect: () async {
                  await model.disconnect();
                  await _startScanner();
                });
                return SingleChildScrollView(
                  padding: EdgeInsets.fromLTRB(wide ? 32 : 18, 18, wide ? 32 : 18, 30),
                  child: Center(
                    child: ConstrainedBox(
                      constraints: const BoxConstraints(maxWidth: 980),
                      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                        const _Header(),
                        const SizedBox(height: 24),
                        if (wide)
                          SizedBox(height: constraints.maxHeight - 130, child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [Expanded(flex: 6, child: scanPanel), const SizedBox(width: 24), Expanded(flex: 5, child: connectionPanel)]))
                        else ...[
                          scanPanel,
                          const SizedBox(height: 22),
                          connectionPanel,
                        ],
                      ]),
                    ),
                  ),
                );
              }),
            ),
          );
        },
      );
}

class _Header extends StatelessWidget {
  const _Header();

  @override
  Widget build(BuildContext context) => const Row(children: [
        AppTesterMark(size: 44),
        SizedBox(width: 12),
        Expanded(child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text('App Tester', style: TextStyle(fontSize: 20, fontWeight: FontWeight.w700)),
          Text('Companion', style: TextStyle(color: Color(0xff8ea6c9))),
        ])),
      ]);
}

class _ScanPanel extends StatelessWidget {
  const _ScanPanel({required this.active, required this.scanning, required this.scanner, required this.onDetect});
  final bool active;
  final bool scanning;
  final MobileScannerController scanner;
  final void Function(BarcodeCapture) onDetect;

  @override
  Widget build(BuildContext context) => AnimatedSwitcher(
        duration: const Duration(milliseconds: 450),
        child: active
            ? const _SuccessVisual(key: ValueKey('active'))
            : Column(key: const ValueKey('scanner'), crossAxisAlignment: CrossAxisAlignment.start, children: [
                Text('Scan connection code', style: Theme.of(context).textTheme.headlineSmall?.copyWith(fontWeight: FontWeight.w700)),
                const SizedBox(height: 6),
                const Text('Open App Tester on your computer, select an app, then choose Connect companion.', style: TextStyle(color: Color(0xffaebfd8), height: 1.4)),
                const SizedBox(height: 18),
                AspectRatio(
                  aspectRatio: 1,
                  child: ClipRRect(
                    borderRadius: BorderRadius.circular(24),
                    child: Stack(fit: StackFit.expand, children: [
                      if (scanning) MobileScanner(controller: scanner, onDetect: onDetect) else const ColoredBox(color: Color(0xff0d1d35)),
                      const _ScannerFrame(),
                    ]),
                  ),
                ),
              ]),
      );
}

class _ScannerFrame extends StatelessWidget {
  const _ScannerFrame();
  @override
  Widget build(BuildContext context) => IgnorePointer(
        child: DecoratedBox(
          decoration: BoxDecoration(border: Border.all(color: const Color(0xff2be0a7), width: 2), borderRadius: BorderRadius.circular(24)),
          child: Center(child: Container(width: 180, height: 180, decoration: BoxDecoration(border: Border.all(color: Colors.white70, width: 2), borderRadius: BorderRadius.circular(18)))),
        ),
      );
}

class _SuccessVisual extends StatelessWidget {
  const _SuccessVisual({super.key});
  @override
  Widget build(BuildContext context) => Container(
        constraints: const BoxConstraints(minHeight: 320),
        decoration: BoxDecoration(gradient: const LinearGradient(colors: [Color(0xff123a36), Color(0xff0b2032)], begin: Alignment.topLeft, end: Alignment.bottomRight), borderRadius: BorderRadius.circular(28)),
        child: Center(child: TweenAnimationBuilder<double>(tween: Tween(begin: 0.7, end: 1), duration: const Duration(milliseconds: 550), curve: Curves.easeOutBack, builder: _buildCheck)),
      );

  static Widget _buildCheck(BuildContext context, double scale, Widget? child) => Transform.scale(scale: scale, child: const Icon(Icons.check_circle_rounded, size: 112, color: Color(0xff2be0a7)));
}

class _ConnectionPanel extends StatelessWidget {
  const _ConnectionPanel({required this.model, required this.active, required this.connected, required this.onDisconnect});
  final ProxySafetyViewModel model;
  final bool active;
  final bool connected;
  final Future<void> Function() onDisconnect;

  @override
  Widget build(BuildContext context) {
    final network = switch (model.networkMatch) {
      NetworkMatch.sameWifi => ('Same Wi-Fi confirmed', Icons.wifi_rounded, const Color(0xff2be0a7)),
      NetworkMatch.reachable => ('Desktop reachable; Wi-Fi name unavailable', Icons.wifi_find_rounded, const Color(0xffffc66d)),
      NetworkMatch.unreachable => ('Not on same reachable network', Icons.wifi_off_rounded, const Color(0xffff879a)),
      NetworkMatch.unknown => ('Wi-Fi not checked yet', Icons.wifi_find_rounded, const Color(0xff8ea6c9)),
    };
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      AnimatedContainer(
        duration: const Duration(milliseconds: 350),
        padding: const EdgeInsets.all(20),
        decoration: BoxDecoration(color: active ? const Color(0xff103831) : const Color(0xff0d1d35), borderRadius: BorderRadius.circular(20), border: Border.all(color: active ? const Color(0xff246f5d) : const Color(0xff203651))),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Icon(active ? Icons.link_rounded : Icons.qr_code_scanner_rounded, color: active ? const Color(0xff2be0a7) : const Color(0xff82baff), size: 34),
          const SizedBox(height: 16),
          Text(active ? 'Capture connected' : connected ? 'Desktop connected' : model.isWorking ? 'Connecting…' : 'Ready to scan', style: Theme.of(context).textTheme.titleLarge?.copyWith(fontWeight: FontWeight.w700)),
          const SizedBox(height: 7),
          Text(active ? 'Traffic is flowing to App Tester. You can leave this screen open or switch apps.' : connected ? 'Waiting for a package selection from App Tester.' : 'Host and port are configured from the QR code.', style: const TextStyle(color: Color(0xffaebfd8), height: 1.4)),
        ]),
      ),
      const SizedBox(height: 14),
      _StatusRow(icon: network.$2, color: network.$3, title: network.$1),
      if (model.status?.targetPackage case final package?) ...[
        const SizedBox(height: 10),
        _StatusRow(icon: Icons.android_rounded, color: const Color(0xff82baff), title: package),
      ],
      if (model.error != null) ...[
        const SizedBox(height: 14),
        _ErrorMessage(model.error!),
      ],
      if (connected) ...[
        const SizedBox(height: 18),
        OutlinedButton.icon(onPressed: model.isWorking ? null : onDisconnect, icon: const Icon(Icons.link_off_rounded), label: const Text('Disconnect from desktop'), style: OutlinedButton.styleFrom(minimumSize: const Size.fromHeight(50))),
      ],
    ]);
  }
}

class _StatusRow extends StatelessWidget {
  const _StatusRow({required this.icon, required this.color, required this.title});
  final IconData icon;
  final Color color;
  final String title;
  @override
  Widget build(BuildContext context) => Container(
        padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 13),
        decoration: BoxDecoration(color: const Color(0xff0a1729), borderRadius: BorderRadius.circular(14)),
        child: Row(children: [Icon(icon, color: color, size: 21), const SizedBox(width: 11), Expanded(child: Text(title, style: const TextStyle(fontWeight: FontWeight.w600)))]),
      );
}

class _ErrorMessage extends StatelessWidget {
  const _ErrorMessage(this.message);
  final String message;
  @override
  Widget build(BuildContext context) => Container(
        padding: const EdgeInsets.all(14),
        decoration: BoxDecoration(color: const Color(0xff421c27), borderRadius: BorderRadius.circular(14)),
        child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [const Icon(Icons.error_outline, color: Color(0xffff9bac)), const SizedBox(width: 10), Expanded(child: Text(message, style: const TextStyle(height: 1.35)))]),
      );
}
