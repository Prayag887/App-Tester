import 'package:flutter/material.dart';

import '../../../shared/brand/app_tester_mark.dart';
import '../data/proxy_safety_repository.dart';
import 'proxy_safety_view_model.dart';

class ProxySafetyScreen extends StatefulWidget {
  const ProxySafetyScreen({required this.viewModel, super.key});

  final ProxySafetyViewModel viewModel;

  @override
  State<ProxySafetyScreen> createState() => _ProxySafetyScreenState();
}

class _ProxySafetyScreenState extends State<ProxySafetyScreen> {
  late final TextEditingController _host;
  late final TextEditingController _port;
  late final TextEditingController _package;

  @override
  void initState() {
    super.initState();
    _host = TextEditingController();
    _port = TextEditingController(text: '8080');
    _package = TextEditingController();
  }

  @override
  void dispose() {
    _host.dispose();
    _port.dispose();
    _package.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
        animation: widget.viewModel,
        builder: (context, _) {
          final model = widget.viewModel;
          final status = model.status;
          if (status?.host != null && _host.text.isEmpty) _host.text = status!.host!;
          if (status?.port != null && _port.text == '8080') _port.text = '${status!.port}';
          if (status?.targetPackage != null && _package.text.isEmpty) _package.text = status!.targetPackage!;
          final active = status?.isVpnActive == true;
          return Scaffold(
            body: SafeArea(
              child: ListView(
                padding: const EdgeInsets.fromLTRB(20, 18, 20, 32),
                children: [
                  const _AppHeader(),
                  const SizedBox(height: 24),
                  _ConnectionCard(active: active, statusMessage: status?.message),
                  const SizedBox(height: 18),
                  Text('Desktop link', style: Theme.of(context).textTheme.titleLarge),
                  const SizedBox(height: 4),
                  const Text('Use the green Desktop host shown in the App Tester desktop header.'),
                  const SizedBox(height: 16),
                  _EndpointFields(host: _host, port: _port, targetPackage: _package),
                  if (model.error != null) ...[
                    const SizedBox(height: 12),
                    _InlineMessage(message: model.error!, isError: true),
                  ],
                  const SizedBox(height: 16),
                  FilledButton.icon(
                    style: FilledButton.styleFrom(minimumSize: const Size.fromHeight(52)),
                    onPressed: model.isWorking
                        ? null
                        : () => active
                            ? model.stopVpn()
                            : model.startVpn(_host.text, _port.text, _package.text),
                    icon: Icon(active ? Icons.stop_circle_outlined : Icons.play_circle_outline),
                    label: Text(model.isWorking
                        ? 'Updating connection…'
                        : active
                            ? 'Stop VPN capture'
                            : 'Start VPN capture'),
                  ),
                  const SizedBox(height: 12),
                  const _PermissionNote(),
                  const SizedBox(height: 28),
                  _ActivityLog(logs: status?.logs ?? const [], active: active),
                ],
              ),
            ),
          );
        },
      );
}

class _AppHeader extends StatelessWidget {
  const _AppHeader();

  @override
  Widget build(BuildContext context) => Row(children: [
        const AppTesterMark(size: 54),
        const SizedBox(width: 14),
        Expanded(child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text('App Tester', style: Theme.of(context).textTheme.headlineSmall?.copyWith(fontWeight: FontWeight.w700)),
          Text('Companion', style: Theme.of(context).textTheme.bodyLarge?.copyWith(color: const Color(0xff7b93b7))),
        ])),
        const Icon(Icons.more_horiz, color: Color(0xff7b93b7)),
      ]);
}

class _ConnectionCard extends StatelessWidget {
  const _ConnectionCard({required this.active, this.statusMessage});
  final bool active;
  final String? statusMessage;

  @override
  Widget build(BuildContext context) => Card(
        child: Padding(
          padding: const EdgeInsets.all(18),
          child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Container(
              width: 42, height: 42,
              decoration: BoxDecoration(color: active ? const Color(0xff113d38) : const Color(0xff162842), borderRadius: BorderRadius.circular(12)),
              child: Icon(active ? Icons.link : Icons.link_off, color: active ? const Color(0xff2be0a7) : const Color(0xff91a5c2)),
            ),
            const SizedBox(width: 14),
            Expanded(child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text(active ? 'Desktop link active' : 'Direct networking', style: Theme.of(context).textTheme.titleMedium?.copyWith(fontWeight: FontWeight.w700)),
              const SizedBox(height: 4),
              Text(statusMessage ?? (active ? 'Checking that your desktop is reachable.' : 'No companion network action is running.'), style: const TextStyle(color: Color(0xffaebfd8))),
            ])),
          ]),
        ),
      );
}

class _EndpointFields extends StatelessWidget {
  const _EndpointFields({required this.host, required this.port, required this.targetPackage});
  final TextEditingController host;
  final TextEditingController port;
  final TextEditingController targetPackage;

  @override
  Widget build(BuildContext context) => Column(children: [
        Row(children: [
          Expanded(flex: 3, child: TextField(controller: host, keyboardType: TextInputType.url, decoration: const InputDecoration(labelText: 'Desktop host', hintText: '192.168.1.24', prefixIcon: Icon(Icons.computer_outlined)))),
          const SizedBox(width: 12),
          Expanded(flex: 2, child: TextField(controller: port, keyboardType: TextInputType.number, decoration: const InputDecoration(labelText: 'Port', prefixIcon: Icon(Icons.settings_ethernet)))),
        ]),
        const SizedBox(height: 12),
        TextField(controller: targetPackage, autocorrect: false, decoration: const InputDecoration(labelText: 'Selected package', hintText: 'com.example.app', prefixIcon: Icon(Icons.apps_outlined))),
      ]);
}

class _PermissionNote extends StatelessWidget {
  const _PermissionNote();
  @override
  Widget build(BuildContext context) => const _InlineMessage(
        message: 'No device-owner or administrator permission is required. Android asks for its standard VPN consent once, and only the selected package is routed through the capture relay.',
      );
}

class _InlineMessage extends StatelessWidget {
  const _InlineMessage({required this.message, this.isError = false});
  final String message;
  final bool isError;
  @override
  Widget build(BuildContext context) => DecoratedBox(
        decoration: BoxDecoration(color: isError ? const Color(0xff421c27) : const Color(0xff102540), borderRadius: BorderRadius.circular(12)),
        child: Padding(padding: const EdgeInsets.all(13), child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Icon(isError ? Icons.error_outline : Icons.info_outline, color: isError ? const Color(0xffff9bac) : const Color(0xff82baff), size: 20),
          const SizedBox(width: 10), Expanded(child: Text(message, style: const TextStyle(color: Color(0xffcad8ea), height: 1.35))),
        ])),
      );
}

class _ActivityLog extends StatelessWidget {
  const _ActivityLog({required this.logs, required this.active});
  final List<CompanionLogEntry> logs;
  final bool active;
  @override
  Widget build(BuildContext context) => Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Row(children: [Text('Activity log', style: Theme.of(context).textTheme.titleLarge), const Spacer(), const Text('LIVE', style: TextStyle(color: Color(0xff2be0a7), fontWeight: FontWeight.w700, fontSize: 12))]),
        const SizedBox(height: 10),
        Card(child: Padding(padding: const EdgeInsets.all(16), child: Column(children: [
          if (logs.isEmpty) _LogRow(icon: active ? Icons.check_circle : Icons.info_outline, color: active ? const Color(0xff2be0a7) : const Color(0xff82baff), title: active ? 'Waiting for the first health check' : 'Companion ready', detail: 'Enter the desktop host to begin.')
          else ...logs.take(8).map((entry) => Padding(padding: const EdgeInsets.only(bottom: 14), child: _LogRow(icon: Icons.circle, color: const Color(0xff2be0a7), title: entry.message, detail: entry.time))),
          const Divider(height: 24, color: Color(0xff203651)),
          const _LogRow(icon: Icons.privacy_tip_outlined, color: Color(0xff91a5c2), title: 'No device administration', detail: 'The companion does not take ownership of this phone.'),
        ]))),
      ]);
}

class _LogRow extends StatelessWidget {
  const _LogRow({required this.icon, required this.color, required this.title, required this.detail});
  final IconData icon; final Color color; final String title; final String detail;
  @override
  Widget build(BuildContext context) => Row(crossAxisAlignment: CrossAxisAlignment.start, children: [Icon(icon, color: color, size: 20), const SizedBox(width: 11), Expanded(child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [Text(title, style: const TextStyle(fontWeight: FontWeight.w600)), const SizedBox(height: 3), Text(detail, style: const TextStyle(color: Color(0xffaebfd8)))]))]);
}
