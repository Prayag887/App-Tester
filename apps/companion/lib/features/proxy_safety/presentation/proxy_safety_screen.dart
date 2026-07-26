import 'package:flutter/material.dart';

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

  @override
  void initState() {
    super.initState();
    _host = TextEditingController();
    _port = TextEditingController(text: '8080');
  }

  @override
  void dispose() {
    _host.dispose();
    _port.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
        animation: widget.viewModel,
        builder: (context, _) {
          final model = widget.viewModel;
          final status = model.status;
          if (status?.host != null && _host.text.isEmpty) {
            _host.text = status!.host!;
          }
          if (status?.port != null && _port.text == '8080') {
            _port.text = '${status!.port}';
          }
          return Scaffold(
            appBar: AppBar(title: const Text('App Tester Companion')),
            body: SafeArea(
              child: ListView(
                padding: const EdgeInsets.all(24),
                children: [
                  Text('Fail-open capture',
                      style: Theme.of(context).textTheme.headlineMedium),
                  const SizedBox(height: 8),
                  const Text(
                      'This device monitors the desktop proxy. After three failed checks, it clears the proxy so other apps return to direct networking.'),
                  const SizedBox(height: 24),
                  _StatusCard(
                      isDeviceOwner: status?.isDeviceOwner ?? false,
                      isArmed: status?.isArmed ?? false,
                      message: status?.message),
                  const SizedBox(height: 24),
                  TextField(
                      controller: _host,
                      keyboardType: TextInputType.url,
                      decoration: const InputDecoration(
                          labelText: 'Desktop host', hintText: '10.10.10.15')),
                  const SizedBox(height: 12),
                  TextField(
                      controller: _port,
                      keyboardType: TextInputType.number,
                      decoration:
                          const InputDecoration(labelText: 'Proxy port')),
                  if (model.error != null) ...[
                    const SizedBox(height: 12),
                    Text(model.error!,
                        style: TextStyle(
                            color: Theme.of(context).colorScheme.error)),
                  ],
                  const SizedBox(height: 24),
                  FilledButton.icon(
                    onPressed: model.isWorking || status?.isDeviceOwner != true
                        ? null
                        : () => model.arm(_host.text, _port.text),
                    icon: const Icon(Icons.shield_outlined),
                    label: Text(model.isWorking
                        ? 'Working…'
                        : 'Start protected capture'),
                  ),
                  const SizedBox(height: 12),
                  OutlinedButton.icon(
                    onPressed: model.isWorking || status?.isArmed != true
                        ? null
                        : model.disarm,
                    icon: const Icon(Icons.public_off_outlined),
                    label: const Text('Stop and restore direct networking'),
                  ),
                  const SizedBox(height: 24),
                  const Text(
                      'Device-owner setup is required because Android reserves global proxy control for managed devices. Do not enable this on a personal device unless you understand device-owner provisioning.'),
                ],
              ),
            ),
          );
        },
      );
}

class _StatusCard extends StatelessWidget {
  const _StatusCard(
      {required this.isDeviceOwner, required this.isArmed, this.message});

  final bool isDeviceOwner;
  final bool isArmed;
  final String? message;

  @override
  Widget build(BuildContext context) => Card(
        child: Padding(
          padding: const EdgeInsets.all(16),
          child:
              Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(
                isArmed
                    ? 'Protected capture is active'
                    : 'Direct networking is active',
                style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 4),
            Text(isDeviceOwner
                ? 'Device-owner permission granted'
                : 'Device-owner permission is required'),
            if (message != null) ...[const SizedBox(height: 8), Text(message!)],
          ]),
        ),
      );
}
