import 'package:flutter/material.dart';

import '../../../shared/brand/app_tester_mark.dart';
import 'proxy_safety_view_model.dart';

class ProxySafetyScreen extends StatefulWidget {
  const ProxySafetyScreen({required this.viewModel, super.key});

  final ProxySafetyViewModel viewModel;

  @override
  State<ProxySafetyScreen> createState() => _ProxySafetyScreenState();
}

class _ProxySafetyScreenState extends State<ProxySafetyScreen> {
  final _host = TextEditingController(text: '127.0.0.1');
  final _port = TextEditingController(text: '8080');
  final _package = TextEditingController();
  String? _loadedEndpoint;

  @override
  void dispose() {
    _host.dispose();
    _port.dispose();
    _package.dispose();
    super.dispose();
  }

  void _restoreSavedSettings() {
    final status = widget.viewModel.status;
    if (status == null) return;
    final endpoint = '${status.host}:${status.port}:${status.targetPackage}';
    if (_loadedEndpoint == endpoint) return;
    _loadedEndpoint = endpoint;
    _host.text = status.host ?? _host.text;
    _port.text = status.port?.toString() ?? _port.text;
    _package.text = status.targetPackage ?? _package.text;
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
        animation: widget.viewModel,
        builder: (context, _) {
          _restoreSavedSettings();
          final model = widget.viewModel;
          final active = model.status?.isVpnActive == true;
          return Scaffold(
            body: SafeArea(
              child: SingleChildScrollView(
                padding: const EdgeInsets.all(24),
                child: Center(
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 520),
                    child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          const Row(children: [
                            AppTesterMark(size: 44),
                            SizedBox(width: 12),
                            Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Text('Companion',
                                      style: TextStyle(
                                          fontSize: 20,
                                          fontWeight: FontWeight.w700)),
                                  Text('Control VPN capture from this phone',
                                      style:
                                          TextStyle(color: Color(0xff8ea6c9))),
                                ]),
                          ]),
                          const SizedBox(height: 30),
                          _StatusCard(
                              active: active,
                              packageName: model.status?.targetPackage),
                          const SizedBox(height: 20),
                          Text('VPN connection',
                              style: Theme.of(context)
                                  .textTheme
                                  .titleMedium
                                  ?.copyWith(fontWeight: FontWeight.w700)),
                          const SizedBox(height: 12),
                          TextField(
                              controller: _host,
                              enabled: !active && !model.isWorking,
                              decoration: const InputDecoration(
                                  labelText: 'Desktop host',
                                  hintText: '192.168.1.20')),
                          const SizedBox(height: 12),
                          TextField(
                              controller: _port,
                              enabled: !active && !model.isWorking,
                              keyboardType: TextInputType.number,
                              decoration: const InputDecoration(
                                  labelText: 'Proxy port', hintText: '8080')),
                          const SizedBox(height: 12),
                          TextField(
                              controller: _package,
                              enabled: !active && !model.isWorking,
                              autocorrect: false,
                              decoration: const InputDecoration(
                                  labelText: 'Target package',
                                  hintText: 'com.example.app')),
                          if (model.error != null) ...[
                            const SizedBox(height: 14),
                            Text(model.error!,
                                style:
                                    const TextStyle(color: Color(0xffff9bac))),
                          ],
                          const SizedBox(height: 22),
                          if (!active)
                            FilledButton.icon(
                              onPressed: model.isWorking
                                  ? null
                                  : () => model.connectVpn(
                                      host: _host.text,
                                      portText: _port.text,
                                      targetPackage: _package.text),
                              icon: const Icon(Icons.vpn_key_rounded),
                              label: const Text('Connect VPN'),
                              style: FilledButton.styleFrom(
                                  minimumSize: const Size.fromHeight(52)),
                            )
                          else
                            FilledButton.icon(
                              onPressed: model.isWorking
                                  ? null
                                  : () => model.stopVpn(),
                              icon: const Icon(Icons.stop_circle_outlined),
                              label: const Text('Stop VPN'),
                              style: FilledButton.styleFrom(
                                  minimumSize: const Size.fromHeight(52),
                                  backgroundColor: const Color(0xff9e3d4d)),
                            ),
                          const SizedBox(height: 10),
                          OutlinedButton.icon(
                            onPressed: model.isWorking
                                ? null
                                : () => model.disconnect(),
                            icon: const Icon(Icons.link_off_rounded),
                            label: const Text('Disconnect desktop'),
                            style: OutlinedButton.styleFrom(
                                minimumSize: const Size.fromHeight(52)),
                          ),
                        ]),
                  ),
                ),
              ),
            ),
          );
        },
      );
}

class _StatusCard extends StatelessWidget {
  const _StatusCard({required this.active, required this.packageName});

  final bool active;
  final String? packageName;

  @override
  Widget build(BuildContext context) => Container(
        padding: const EdgeInsets.all(24),
        decoration: BoxDecoration(
          color: active ? const Color(0xff103831) : const Color(0xff0d1d35),
          borderRadius: BorderRadius.circular(22),
          border: Border.all(
              color:
                  active ? const Color(0xff246f5d) : const Color(0xff203651)),
        ),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Icon(active ? Icons.shield_rounded : Icons.phone_android_rounded,
              size: 38,
              color:
                  active ? const Color(0xff2be0a7) : const Color(0xff82baff)),
          const SizedBox(height: 14),
          Text(active ? 'VPN capture active' : 'VPN capture is off',
              style: Theme.of(context)
                  .textTheme
                  .titleLarge
                  ?.copyWith(fontWeight: FontWeight.w700)),
          const SizedBox(height: 7),
          Text(
              active
                  ? 'Capturing traffic from ${packageName ?? 'the selected package'}.'
                  : 'Enter the desktop connection and package details, then start VPN capture.',
              style: const TextStyle(color: Color(0xffaebfd8), height: 1.4)),
        ]),
      );
}
